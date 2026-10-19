//! Audio side of the visualizer: capture from the default input device, FFT, bands.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat, SizedSample, StreamConfig};
use inlay_core::bands::{BandMap, hann_window, settle};
use inlay_core::config::VisualizerCfg;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

/// Same FFT size as the Windows visualizer (`crates/app/src/widgets/visualizer.rs`).
const FFT_SIZE: usize = 2048;
/// New samples needed before another FFT: a quarter of the window.
const HOP: usize = FFT_SIZE / 4;
/// A callback quieter than this peak is silence (`SILENCE` in `crates/app/src/capture.rs`).
const SILENCE: f32 = 1e-4;
/// Sound counts as arriving for this long after the last audible callback.
const ACTIVE_FOR: Duration = Duration::from_secs(1);
/// Bars below this are at rest, so no more redraws (`AT_REST` in the Windows visualizer).
const AT_REST: f32 = 0.003;
/// Longest step the bar physics will take (`MAX_STEP_MS` in the Windows visualizer).
const MAX_STEP_MS: f32 = 100.0;

/// Live spectrum levels from the default input device. On Linux and macOS there is no
/// way to capture one app's audio without extra software, so this listens to whatever
/// the system's default input is: point it at a monitor of the speakers (PulseAudio /
/// PipeWire "Monitor of …", or a loopback device such as BlackHole on macOS).
///
/// It owns the `cpal::Stream`, which is not known to be `Send`, so keep it on the
/// thread that created it (the UI thread).
pub struct Spectrum {
    levels: Arc<Mutex<Vec<f32>>>,
    /// Milliseconds since `start` of the last audible callback, plus one; 0 is never.
    last_loud: Arc<AtomicU64>,
    started: Instant,
    _stream: cpal::Stream,
}

impl Spectrum {
    /// Starts capture. `None` when there is no input device or it cannot be opened (logged).
    /// `wake` is called at most `cfg.fps` times a second while sound is coming in, so the
    /// visualizer window redraws only then.
    pub fn start(cfg: &VisualizerCfg, wake: Arc<dyn Fn() + Send + Sync>) -> Option<Self> {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            eprintln!("audio: no default input device");
            return None;
        };
        let supported = match device.default_input_config() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("audio: no input config: {error}");
                return None;
            }
        };
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();

        let bars = cfg.bars as usize;
        let levels = Arc::new(Mutex::new(vec![0.0; bars]));
        let last_loud = Arc::new(AtomicU64::new(0));
        let started = Instant::now();
        let capture = Capture::new(
            cfg,
            config.sample_rate as f32,
            usize::from(config.channels),
            levels.clone(),
            last_loud.clone(),
            started,
            wake,
        );

        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, capture),
            SampleFormat::I16 => build::<i16>(&device, config, capture),
            SampleFormat::I32 => build::<i32>(&device, config, capture),
            SampleFormat::U16 => build::<u16>(&device, config, capture),
            other => {
                eprintln!("audio: unsupported sample format {other}");
                return None;
            }
        };
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("audio: could not open the input device: {error}");
                return None;
            }
        };
        if let Err(error) = stream.play() {
            eprintln!("audio: could not start capture: {error}");
            return None;
        }
        Some(Self {
            levels,
            last_loud,
            started,
            _stream: stream,
        })
    }

    /// The current level of each bar, 0.0 to 1.0, `cfg.bars` long.
    pub fn levels(&self) -> Vec<f32> {
        self.levels.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Whether any sound arrived in the last second.
    pub fn active(&self) -> bool {
        let last = self.last_loud.load(Ordering::Relaxed);
        last != 0 && self.started.elapsed() < Duration::from_millis(last - 1) + ACTIVE_FOR
    }
}

fn build<T: SizedSample>(
    device: &cpal::Device,
    config: StreamConfig,
    mut capture: Capture,
) -> Result<cpal::Stream, cpal::Error>
where
    f32: cpal::FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _: &_| capture.push(data),
        |error| eprintln!("audio: stream error: {error}"),
        None,
    )
}

/// Samples to levels: window, FFT, band map. Everything is allocated up front.
struct Dsp {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    norm: f32,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,
    map: BandMap,
    gain_db: f32,
}

impl Dsp {
    fn new(cfg: &VisualizerCfg, sample_rate: f32) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let (window, norm) = hann_window(FFT_SIZE);
        Self {
            scratch: vec![Complex::default(); fft.get_inplace_scratch_len()],
            fft,
            window,
            norm,
            spectrum: vec![Complex::default(); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            map: BandMap::new(
                cfg.bars as usize,
                FFT_SIZE,
                sample_rate,
                cfg.min_hz,
                cfg.max_hz,
                cfg.tilt,
            ),
            gain_db: cfg.sensitivity,
        }
    }

    /// Writes one level per bar for the newest `FFT_SIZE` mono samples (oldest first).
    fn targets(&mut self, mono: &[f32], out: &mut [f32]) {
        for ((bin, &sample), &weight) in self.spectrum.iter_mut().zip(mono).zip(&self.window) {
            *bin = Complex::new(sample * weight, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);
        for (magnitude, bin) in self.magnitudes.iter_mut().zip(&self.spectrum) {
            *magnitude = bin.norm() * self.norm;
        }
        self.map.levels(&self.magnitudes, self.gain_db, out);
    }
}

/// Everything the audio callback owns.
struct Capture {
    dsp: Dsp,
    channels: usize,
    ring: Vec<f32>,
    /// Oldest-first copy of the ring, for the FFT.
    ordered: Vec<f32>,
    head: usize,
    fresh: usize,
    targets: Vec<f32>,
    smooth: Vec<f32>,
    attack_ms: f32,
    decay_ms: f32,
    frame: Duration,
    last_run: Instant,
    peak: f32,
    shared: Arc<Mutex<Vec<f32>>>,
    last_loud: Arc<AtomicU64>,
    started: Instant,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Capture {
    fn new(
        cfg: &VisualizerCfg,
        sample_rate: f32,
        channels: usize,
        shared: Arc<Mutex<Vec<f32>>>,
        last_loud: Arc<AtomicU64>,
        started: Instant,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let bars = cfg.bars as usize;
        Self {
            dsp: Dsp::new(cfg, sample_rate),
            channels: channels.max(1),
            ring: vec![0.0; FFT_SIZE],
            ordered: vec![0.0; FFT_SIZE],
            head: 0,
            fresh: 0,
            targets: vec![0.0; bars],
            smooth: vec![0.0; bars],
            attack_ms: cfg.attack_ms,
            decay_ms: cfg.decay_ms,
            frame: Duration::from_secs_f32(1.0 / cfg.fps.max(1) as f32),
            last_run: Instant::now(),
            peak: 0.0,
            shared,
            last_loud,
            started,
            wake,
        }
    }

    /// Called by cpal with interleaved samples of any supported format.
    fn push<T: Sample>(&mut self, data: &[T])
    where
        f32: cpal::FromSample<T>,
    {
        for frame in data.chunks_exact(self.channels) {
            let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
            let mono = sum / self.channels as f32;
            self.peak = self.peak.max(mono.abs());
            self.ring[self.head] = mono;
            self.head = (self.head + 1) % FFT_SIZE;
            self.fresh += 1;
        }

        let now = Instant::now();
        if self.fresh < HOP || now.duration_since(self.last_run) < self.frame {
            return;
        }
        let dt_ms = (now.duration_since(self.last_run).as_secs_f32() * 1000.0).min(MAX_STEP_MS);
        self.last_run = now;
        self.fresh = 0;

        let (newest, oldest) = self.ring.split_at(self.head);
        self.ordered[..oldest.len()].copy_from_slice(oldest);
        self.ordered[oldest.len()..].copy_from_slice(newest);
        self.dsp.targets(&self.ordered, &mut self.targets);

        let heard = self.peak > SILENCE;
        self.peak = 0.0;
        if heard {
            let ms = now.duration_since(self.started).as_millis() as u64;
            self.last_loud.store(ms + 1, Ordering::Relaxed);
        }
        for (level, &target) in self.smooth.iter_mut().zip(&self.targets) {
            *level = settle(*level, target, dt_ms, self.attack_ms, self.decay_ms);
        }
        if let Ok(mut shared) = self.shared.lock() {
            shared.clone_from(&self.smooth);
        }
        // Redraw while sound comes in, and while the bars are still falling after it.
        if heard || self.smooth.iter().any(|&l| l >= AT_REST) {
            (self.wake)();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn cfg() -> VisualizerCfg {
        VisualizerCfg {
            bars: 32,
            min_hz: 40.0,
            max_hz: 16_000.0,
            sensitivity: 0.0,
            tilt: 0.0,
            ..VisualizerCfg::default()
        }
    }

    #[test]
    fn silence_gives_zero_levels() {
        let mut dsp = Dsp::new(&cfg(), RATE);
        let mut out = vec![1.0; 32];
        dsp.targets(&vec![0.0; FFT_SIZE], &mut out);
        assert!(out.iter().all(|&l| l < 0.01));
    }

    #[test]
    fn a_1khz_sine_lights_its_bar() {
        let mut dsp = Dsp::new(&cfg(), RATE);
        let sine: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1000.0 * i as f32 / RATE).sin())
            .collect();
        let mut out = vec![0.0; 32];
        dsp.targets(&sine, &mut out);
        // 1 kHz sits at log(1000/40) / log(16000/40) of the way up 32 bars: bar 17.
        let loudest = (0..32)
            .max_by(|&a, &b| out[a].total_cmp(&out[b]))
            .unwrap_or(0);
        assert!((16..=18).contains(&loudest), "loudest bar was {loudest}");
        assert!(out[loudest] > out[2] + 0.3 && out[loudest] > out[30] + 0.3);
    }
}
