//! Audio capture for the visualizer: WASAPI loopback, either on Spotify's
//! process tree alone or on everything the PC plays.
//!
//! The capture thread sleeps on events. It wakes when Windows has audio for
//! it, or when the app changes what it should be listening to.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use inlay_core::config::AudioSource;
use windows::Win32::Foundation::{CloseHandle, E_FAIL, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    CreateEventW, INFINITE, SetEvent, WaitForMultipleObjects, WaitForSingleObject,
};
use windows::core::{Error, HRESULT, Interface, Ref, Result, implement};

use crate::window::WM_APP_AUDIO;
use crate::{Notify, log};

/// Samples kept. The visualizer reads the newest 2048 of them.
const RING: usize = 4096;
/// A packet quieter than this is treated as silence.
const SILENCE: f32 = 1e-4;
/// How long after the last audible packet the stream counts as idle.
const IDLE_AFTER: Duration = Duration::from_millis(400);
const SAMPLE_RATE: u32 = 48_000;

const WAVE_FORMAT_PCM: u16 = 1;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const VT_BLOB: u16 = 65;
const BUFFER_SILENT: u32 = 2;

struct Ring {
    samples: Vec<f32>,
    head: usize,
    sample_rate: u32,
}

pub struct Audio {
    ring: Mutex<Ring>,
    /// Sound arrived within the last few hundred milliseconds.
    pub active: AtomicBool,
    /// `None` turns capture off.
    wanted: Mutex<Option<AudioSource>>,
    /// Bumped when `wanted` changes, so a running stream knows to stop.
    generation: AtomicU64,
    /// Event handle that wakes the capture thread.
    wake: isize,
}

impl Audio {
    fn new(wake: isize) -> Self {
        Self {
            ring: Mutex::new(Ring {
                samples: vec![0.0; RING],
                head: 0,
                sample_rate: SAMPLE_RATE,
            }),
            active: AtomicBool::new(false),
            wanted: Mutex::new(None),
            generation: AtomicU64::new(0),
            wake,
        }
    }

    /// An `Audio` with no capture thread behind it, fed by hand.
    pub fn offline() -> Self {
        Self::new(0)
    }

    /// Copies the newest `out.len()` mono samples, oldest first, and returns
    /// their sample rate.
    pub fn latest(&self, out: &mut [f32]) -> u32 {
        let Ok(ring) = self.ring.lock() else {
            out.fill(0.0);
            return SAMPLE_RATE;
        };
        let start = ring.head + RING - out.len().min(RING);
        for (i, sample) in out.iter_mut().enumerate() {
            *sample = ring.samples[(start + i) % RING];
        }
        ring.sample_rate
    }

    pub fn set_source(&self, source: Option<AudioSource>) {
        if let Ok(mut wanted) = self.wanted.lock()
            && *wanted != source
        {
            *wanted = source;
            self.generation.fetch_add(1, Ordering::SeqCst);
            self.kick();
        }
    }

    /// Asks the capture thread to look again: Spotify started or restarted.
    pub fn kick(&self) {
        let _ = unsafe { SetEvent(self.wake_handle()) };
    }

    fn wake_handle(&self) -> HANDLE {
        HANDLE(self.wake as *mut c_void)
    }

    pub fn push(&self, mono: &[f32], sample_rate: u32) {
        if let Ok(mut ring) = self.ring.lock() {
            ring.sample_rate = sample_rate;
            for &sample in mono {
                let head = ring.head;
                ring.samples[head] = sample;
                ring.head = (head + 1) % RING;
            }
        }
    }
}

pub fn spawn(notify: Notify) -> Result<Arc<Audio>> {
    let wake = unsafe { CreateEventW(None, false, false, None)? };
    let audio = Arc::new(Audio::new(wake.0 as isize));
    let shared = audio.clone();
    std::thread::spawn(move || run(&shared, notify));
    Ok(audio)
}

fn run(audio: &Audio, notify: Notify) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    loop {
        let generation = audio.generation.load(Ordering::SeqCst);
        let wanted = audio.wanted.lock().map(|w| *w).unwrap_or(None);
        let opened = match wanted {
            None => None,
            Some(AudioSource::System) => Some(open_system()),
            // No Spotify process yet: sleep until the media session kicks us.
            Some(AudioSource::Spotify) => spotify_pid().map(open_process),
        };
        match opened {
            Some(Ok(stream)) => {
                if let Err(error) = pump(audio, &stream, generation, notify) {
                    log(&format!("audio capture stopped: {error}"));
                }
                unsafe {
                    let _ = stream.client.Stop();
                    let _ = CloseHandle(stream.event);
                }
            }
            Some(Err(error)) => log(&format!("audio capture could not start: {error}")),
            None => {}
        }
        audio.active.store(false, Ordering::SeqCst);
        if audio.generation.load(Ordering::SeqCst) == generation {
            unsafe { WaitForSingleObject(audio.wake_handle(), INFINITE) };
        }
    }
}

struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    channels: usize,
    float: bool,
    sample_rate: u32,
    /// Set when capturing one process, so a restart of Spotify is noticed.
    pid: Option<u32>,
}

/// The executable this app follows. Only the `--probe` diagnostic changes
/// it, to test the capture and media-session code against another app.
static TARGET_EXE: OnceLock<String> = OnceLock::new();

pub fn set_target_exe(name: String) {
    let _ = TARGET_EXE.set(name);
}

pub fn target_exe() -> &'static str {
    TARGET_EXE.get().map_or("Spotify.exe", String::as_str)
}

/// The Spotify process at the root of its tree: Spotify runs several
/// processes and the audio comes from a child of the first.
pub fn spotify_pid() -> Option<u32> {
    let target = target_exe();
    let mut found: Vec<(u32, u32)> = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let name = entry.szExeFile;
            let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
            if String::from_utf16_lossy(&name[..len]).eq_ignore_ascii_case(target) {
                found.push((entry.th32ProcessID, entry.th32ParentProcessID));
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    found
        .iter()
        .find(|(_, parent)| !found.iter().any(|(pid, _)| pid == parent))
        .map(|&(pid, _)| pid)
}

/// The layout of a `PROPVARIANT` holding a `VT_BLOB`, which is how the
/// loopback parameters are handed to `ActivateAudioInterfaceAsync`.
#[repr(C)]
struct BlobVariant {
    vt: u16,
    reserved: [u16; 3],
    size: u32,
    data: *const u8,
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Activated {
    /// Event handle signalled when activation finishes.
    done: isize,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for Activated_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> Result<()> {
        unsafe { SetEvent(HANDLE(self.done as *mut c_void)) }
    }
}

/// Captures only what `pid` and its child processes play.
fn open_process(pid: u32) -> Result<Stream> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let variant = BlobVariant {
        vt: VT_BLOB,
        reserved: [0; 3],
        size: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
        data: (&raw const params).cast(),
    };

    let client: IAudioClient = unsafe {
        let done = CreateEventW(None, false, false, None)?;
        let handler: IActivateAudioInterfaceCompletionHandler = Activated {
            done: done.0 as isize,
        }
        .into();
        let operation = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some((&raw const variant).cast()),
            &handler,
        );
        // On a timeout the handle is leaked on purpose: the handler may still signal it.
        if operation.is_ok() && WaitForSingleObject(done, 5000) == WAIT_OBJECT_0 {
            let _ = CloseHandle(done);
        }

        let mut status = HRESULT(0);
        let mut activated = None;
        operation?.GetActivateResult(&mut status, &mut activated)?;
        status.ok()?;
        activated.ok_or_else(|| Error::from(E_FAIL))?.cast()?
    };

    // Process loopback has no mix format to ask for: the caller names one and
    // Windows converts. Float first, 16-bit as the fallback.
    for float in [true, false] {
        let bits: u16 = if float { 32 } else { 16 };
        let format = WAVEFORMATEX {
            wFormatTag: if float {
                WAVE_FORMAT_IEEE_FLOAT
            } else {
                WAVE_FORMAT_PCM
            },
            nChannels: 2,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * u32::from(bits) / 4,
            nBlockAlign: bits / 4,
            wBitsPerSample: bits,
            cbSize: 0,
        };
        let initialized = unsafe {
            client.Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
                0,
                0,
                &format,
                None,
            )
        };
        if initialized.is_ok() {
            return start(client, 2, float, SAMPLE_RATE, Some(pid));
        }
    }
    Err(Error::from(E_FAIL))
}

/// Captures the default output device: everything the PC plays.
fn open_system() -> Result<Stream> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let client: IAudioClient = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)?
            .Activate(CLSCTX_ALL, None)?;

        let format = client.GetMixFormat()?;
        // WAVEFORMATEX is packed, so its fields are copied out, never borrowed.
        let (channels, sample_rate, bits) = (
            (*format).nChannels,
            (*format).nSamplesPerSec,
            (*format).wBitsPerSample,
        );
        let initialized = client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            0,
            0,
            format,
            None,
        );
        CoTaskMemFree(Some(format.cast()));
        initialized?;

        // The shared mix format is 32-bit float on every current Windows.
        if (bits != 32 && bits != 16) || channels == 0 {
            return Err(Error::from(E_FAIL));
        }
        start(client, channels as usize, bits == 32, sample_rate, None)
    }
}

fn start(
    client: IAudioClient,
    channels: usize,
    float: bool,
    sample_rate: u32,
    pid: Option<u32>,
) -> Result<Stream> {
    unsafe {
        let event = CreateEventW(None, false, false, None)?;
        client.SetEventHandle(event)?;
        let capture = client.GetService()?;
        client.Start()?;
        Ok(Stream {
            client,
            capture,
            event,
            channels,
            float,
            sample_rate,
            pid,
        })
    }
}

/// Mixes one packet of interleaved frames down to mono.
fn to_mono(bytes: &[u8], channels: usize, float: bool, out: &mut Vec<f32>) {
    out.clear();
    let sample_bytes = if float { 4 } else { 2 };
    for frame in bytes.chunks_exact(channels * sample_bytes) {
        let sum: f32 = if float {
            frame
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .sum()
        } else {
            frame
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes(*b)) / 32_768.0)
                .sum()
        };
        out.push(sum / channels as f32);
    }
}

fn pump(audio: &Audio, stream: &Stream, generation: u64, notify: Notify) -> Result<()> {
    let handles = [stream.event, audio.wake_handle()];
    let frame_bytes = stream.channels * if stream.float { 4 } else { 2 };
    let mut mono = Vec::new();
    let mut last_loud = Instant::now();

    loop {
        // While idle there is nothing to time out: sleep until audio arrives.
        let timeout = if audio.active.load(Ordering::SeqCst) {
            IDLE_AFTER.as_millis() as u32
        } else {
            INFINITE
        };
        let waited = unsafe { WaitForMultipleObjects(&handles, false, timeout) };
        if waited.0 == WAIT_OBJECT_0.0 + 1 {
            let reconfigured = audio.generation.load(Ordering::SeqCst) != generation;
            let restarted = stream.pid.is_some_and(|pid| spotify_pid() != Some(pid));
            if reconfigured || restarted {
                return Ok(());
            }
            continue;
        }

        let mut peak = 0.0f32;
        unsafe {
            while stream.capture.GetNextPacketSize()? > 0 {
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags) = (0, 0);
                stream
                    .capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                if flags & BUFFER_SILENT == 0 && !data.is_null() {
                    // SAFETY: WASAPI guarantees `frames` whole frames at `data`
                    // until ReleaseBuffer.
                    let bytes = std::slice::from_raw_parts(data, frames as usize * frame_bytes);
                    to_mono(bytes, stream.channels, stream.float, &mut mono);
                    peak = mono.iter().fold(peak, |p, s| p.max(s.abs()));
                    audio.push(&mono, stream.sample_rate);
                }
                stream.capture.ReleaseBuffer(frames)?;
            }
        }

        if peak > SILENCE {
            last_loud = Instant::now();
            if !audio.active.swap(true, Ordering::SeqCst) {
                notify.post(WM_APP_AUDIO);
            }
        } else if last_loud.elapsed() > IDLE_AFTER {
            audio.active.store(false, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixes_float_stereo_to_mono() {
        let mut bytes = Vec::new();
        for sample in [0.5f32, -0.25, 1.0, 1.0] {
            bytes.extend(sample.to_le_bytes());
        }
        let mut mono = Vec::new();
        to_mono(&bytes, 2, true, &mut mono);
        assert_eq!(mono, [0.125, 1.0]);
    }

    #[test]
    fn mixes_16_bit_stereo_to_mono() {
        let mut bytes = Vec::new();
        for sample in [16_384i16, 16_384, -32_768, 0] {
            bytes.extend(sample.to_le_bytes());
        }
        let mut mono = Vec::new();
        to_mono(&bytes, 2, false, &mut mono);
        assert_eq!(mono, [0.5, -0.5]);
    }

    #[test]
    fn the_blob_variant_matches_the_propvariant_layout() {
        assert_eq!(size_of::<BlobVariant>(), 24);
        assert_eq!(std::mem::offset_of!(BlobVariant, size), 8);
        assert_eq!(std::mem::offset_of!(BlobVariant, data), 16);
    }
}
