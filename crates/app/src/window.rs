//! Window classes, the window procedure, and z-order helpers.
//!
//! The window procedure never touches app state. It turns messages into
//! [`Event`]s on a queue that the main loop drains, which keeps Windows'
//! re-entrant calls (a `SetWindowPos` that synchronously sends `WM_SIZE`)
//! away from anything that is borrowed.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::c_void;

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, Error, PCWSTR, Result, w};

pub const WM_APP_MEDIA: u32 = WM_APP + 1;
pub const WM_APP_LYRICS: u32 = WM_APP + 2;
pub const WM_APP_AUDIO: u32 = WM_APP + 3;
pub const WM_APP_TRAY: u32 = WM_APP + 4;
pub const WM_APP_SHOW_SETTINGS: u32 = WM_APP + 5;
/// The `windows` crate keeps this one under the Controls feature.
const WM_MOUSELEAVE: u32 = 0x02A3;

pub const CLASS: PCWSTR = w!("Inlay");
pub const MAIN_TITLE: PCWSTR = w!("Inlay");

/// What a window is for, stored in its user data.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(isize)]
pub enum Role {
    /// Hidden. Owns the tray icon, hotkeys and cross-thread notifications.
    Main = 0,
    Widget = 1,
    Panel = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    Down,
    Up,
    Move,
    Leave,
}

#[derive(Debug, Clone, Copy)]
pub enum Event {
    Media,
    Lyrics,
    Audio,
    ShowSettings,
    TaskbarCreated,
    /// Resolution, DPI or work area changed.
    Display,
    Tray(u32),
    Hotkey(i32),
    Foreground(HWND),
    Mouse {
        hwnd: HWND,
        kind: Mouse,
        x: i32,
        y: i32,
    },
    Wheel {
        hwnd: HWND,
        delta: i32,
    },
    /// A typed character, as a UTF-16 code unit.
    Char {
        hwnd: HWND,
        code: u32,
    },
    /// The user finished dragging or resizing a window.
    Moved(HWND),
    /// A window stopped being the active one.
    Blur(HWND),
    Close(HWND),
}

thread_local! {
    static EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
    static EDIT: Cell<bool> = const { Cell::new(false) };
    static PIN_BOTTOM: Cell<bool> = const { Cell::new(false) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
    /// Boxes, in each widget window's own pixels, that take the mouse in
    /// edit mode instead of moving the whole window.
    static PARTS: RefCell<Vec<(isize, RECT)>> = const { RefCell::new(Vec::new()) };
    /// Widget windows sized by their contents, which have no edges to drag.
    static FITTED: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

fn push(event: Event) {
    EVENTS.with_borrow_mut(|queue| queue.push_back(event));
}

pub fn next_event() -> Option<Event> {
    EVENTS.with_borrow_mut(VecDeque::pop_front)
}

/// In edit mode widget windows answer hit tests as a caption and resize
/// borders, so Windows itself moves and sizes them.
pub fn set_edit_mode(on: bool) {
    EDIT.set(on);
}

/// Replaces the draggable boxes of one widget window.
/// `fitted` marks a window whose size follows its elements.
pub fn set_parts(hwnd: HWND, boxes: &[RECT], fitted: bool) {
    let key = hwnd.0 as isize;
    FITTED.with_borrow_mut(|windows| {
        windows.retain(|owner| *owner != key);
        if fitted {
            windows.push(key);
        }
    });
    PARTS.with_borrow_mut(|parts| {
        parts.retain(|(owner, _)| *owner != key);
        parts.extend(boxes.iter().map(|r| (key, *r)));
    });
}

fn on_part(hwnd: HWND, x: i32, y: i32) -> bool {
    let key = hwnd.0 as isize;
    PARTS.with_borrow(|parts| {
        parts.iter().any(|(owner, r)| {
            *owner == key && x >= r.left && x < r.right && y >= r.top && y < r.bottom
        })
    })
}

/// While set, widget windows refuse to leave the bottom of the z-order.
pub fn set_pin_bottom(on: bool) {
    PIN_BOTTOM.set(on);
}

fn lparam_point(lp: LPARAM) -> (i32, i32) {
    (
        (lp.0 & 0xFFFF) as i16 as i32,
        ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
    )
}

/// `HTTRANSPARENT` (-1), in the unsigned type of the other hit-test codes.
const HT_THROUGH: u32 = u32::MAX;

fn edit_hit_test(hwnd: HWND, (x, y): (i32, i32)) -> u32 {
    let mut r = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut r) }.is_err() {
        return HTCAPTION;
    }
    // An element of the widget: the app drags that alone.
    if on_part(hwnd, x - r.left, y - r.top) {
        return HTCLIENT;
    }
    // A widget made only of its elements has nothing else to grab: the
    // space between them belongs to whatever window of ours is underneath.
    if FITTED.with_borrow(|windows| windows.contains(&(hwnd.0 as isize))) {
        return HT_THROUGH;
    }
    let border = (12 * dpi(hwnd) / 96) as i32;
    let (left, right) = (x < r.left + border, x >= r.right - border);
    let (top, bottom) = (y < r.top + border, y >= r.bottom - border);
    match (left, right, top, bottom) {
        (true, _, true, _) => HTTOPLEFT,
        (_, true, true, _) => HTTOPRIGHT,
        (true, _, _, true) => HTBOTTOMLEFT,
        (_, true, _, true) => HTBOTTOMRIGHT,
        (true, ..) => HTLEFT,
        (_, true, ..) => HTRIGHT,
        (_, _, true, _) => HTTOP,
        (_, _, _, true) => HTBOTTOM,
        // Anywhere else moves the whole widget.
        _ => HTCAPTION,
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let is_widget = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } == Role::Widget as isize;
    let mouse = |kind| {
        let (x, y) = lparam_point(lp);
        push(Event::Mouse { hwnd, kind, x, y });
    };

    match msg {
        WM_APP_MEDIA => push(Event::Media),
        WM_APP_LYRICS => push(Event::Lyrics),
        WM_APP_AUDIO => push(Event::Audio),
        WM_APP_SHOW_SETTINGS => push(Event::ShowSettings),
        WM_APP_TRAY => push(Event::Tray(lp.0 as u32)),
        WM_HOTKEY => push(Event::Hotkey(wp.0 as i32)),
        WM_DISPLAYCHANGE | WM_DPICHANGED => push(Event::Display),
        WM_SETTINGCHANGE if wp.0 == SPI_SETWORKAREA.0 as usize => push(Event::Display),
        WM_EXITSIZEMOVE => push(Event::Moved(hwnd)),
        // The low word is the new state; zero is WA_INACTIVE.
        WM_ACTIVATE if wp.0 & 0xFFFF == 0 => push(Event::Blur(hwnd)),
        WM_CHAR => push(Event::Char {
            hwnd,
            code: wp.0 as u32,
        }),
        WM_MOUSEWHEEL => push(Event::Wheel {
            hwnd,
            delta: (wp.0 >> 16) as i16 as i32,
        }),
        WM_LBUTTONDOWN => {
            unsafe { SetCapture(hwnd) };
            mouse(Mouse::Down);
        }
        WM_LBUTTONUP => {
            // Fails only when nothing was captured, which is fine.
            let _ = unsafe { ReleaseCapture() };
            mouse(Mouse::Up);
        }
        WM_MOUSEMOVE => {
            let mut track = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = unsafe { TrackMouseEvent(&mut track) };
            mouse(Mouse::Move);
        }
        WM_MOUSELEAVE => push(Event::Mouse {
            hwnd,
            kind: Mouse::Leave,
            x: 0,
            y: 0,
        }),
        WM_CLOSE => {
            push(Event::Close(hwnd));
            return LRESULT(0);
        }
        WM_ERASEBKGND => return LRESULT(1),
        WM_NCHITTEST if is_widget && EDIT.get() => {
            // Through i32, so that `HT_THROUGH` arrives as -1.
            return LRESULT(edit_hit_test(hwnd, lparam_point(lp)) as i32 as isize);
        }
        WM_GETMINMAXINFO if is_widget => {
            // SAFETY: for this message lparam points at a MINMAXINFO owned by the caller.
            let info = unsafe { &mut *(lp.0 as *mut MINMAXINFO) };
            let scale = dpi(hwnd) as i32;
            info.ptMinTrackSize = POINT {
                x: 80 * scale / 96,
                y: 40 * scale / 96,
            };
            return LRESULT(0);
        }
        WM_WINDOWPOSCHANGING if is_widget && PIN_BOTTOM.get() => {
            // SAFETY: for this message lparam points at a WINDOWPOS owned by the caller.
            let pos = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
            pos.hwndInsertAfter = HWND_BOTTOM;
            pos.flags &= !SWP_NOZORDER;
        }
        other if other != 0 && other == TASKBAR_CREATED.get() => push(Event::TaskbarCreated),
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    push(Event::Foreground(hwnd));
}

/// Registers the window class and the system-wide hooks. Call once.
pub fn register() -> Result<()> {
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            // The exe's own icon, resource 1 of `icon.res`, for the settings
            // window's title bar and taskbar button. A test build has no
            // resources, and gets none.
            hIcon: LoadIconW(Some(instance), PCWSTR(std::ptr::without_provenance(1)))
                .unwrap_or_default(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return Err(Error::from(E_FAIL));
        }
        // Explorer broadcasts this when it restarts and the tray icon is gone.
        TASKBAR_CREATED.set(RegisterWindowMessageW(w!("TaskbarCreated")));
        // Out of context: the callback arrives through this thread's message loop.
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
    }
    Ok(())
}

fn create(
    role: Role,
    ex_style: WINDOW_EX_STYLE,
    style: WINDOW_STYLE,
    title: PCWSTR,
    (x, y, w, h): (i32, i32, i32, i32),
) -> Result<HWND> {
    unsafe {
        let hwnd = CreateWindowExW(
            ex_style,
            CLASS,
            title,
            style,
            x,
            y,
            w,
            h,
            None,
            None,
            Some(GetModuleHandleW(None)?.into()),
            None,
        )?;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, role as isize);
        Ok(hwnd)
    }
}

pub fn create_main() -> Result<HWND> {
    create(
        Role::Main,
        WS_EX_TOOLWINDOW,
        WS_POPUP,
        MAIN_TITLE,
        (0, 0, 0, 0),
    )
}

/// A transparent, borderless, never-activated window with no redirection
/// surface: its pixels come only from DirectComposition.
pub fn create_widget(bounds: (i32, i32, i32, i32)) -> Result<HWND> {
    let hwnd = create(
        Role::Widget,
        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
        WS_POPUP,
        w!("Inlay widget"),
        bounds,
    )?;
    // A layered window is invisible until its attributes are set once.
    unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA)? };
    Ok(hwnd)
}

/// An ordinary captioned window, for the settings panel.
pub fn create_panel(
    title: PCWSTR,
    style: WINDOW_STYLE,
    bounds: (i32, i32, i32, i32),
) -> Result<HWND> {
    create(Role::Panel, WS_EX_NOREDIRECTIONBITMAP, style, title, bounds)
}

/// A borderless window above everything, for the tray menu. Unlike a widget
/// it takes focus, which is how it learns that the user clicked elsewhere.
pub fn create_menu(bounds: (i32, i32, i32, i32)) -> Result<HWND> {
    create(
        Role::Panel,
        WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
        WS_POPUP,
        w!("Inlay menu"),
        bounds,
    )
}

/// Where the pointer is on the screen, in pixels.
pub fn pointer() -> (i32, i32) {
    let mut at = POINT::default();
    // On failure the point stays at the origin, which is still on screen.
    let _ = unsafe { GetCursorPos(&mut at) };
    (at.x, at.y)
}

/// Click-through windows let every mouse event fall to whatever is beneath.
pub fn set_click_through(hwnd: HWND, on: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let bit = WS_EX_TRANSPARENT.0 as isize;
        let wanted = if on { ex | bit } else { ex & !bit };
        if wanted != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Z {
    Bottom,
    Normal,
    Top,
}

pub fn set_z(hwnd: HWND, z: Z) {
    let after = match z {
        Z::Bottom => HWND_BOTTOM,
        Z::Normal => HWND_NOTOPMOST,
        Z::Top => HWND_TOPMOST,
    };
    let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
    // Failure here means the window is gone; there is nothing to recover.
    let _ = unsafe { SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, flags) };
}

pub fn set_bounds(hwnd: HWND, (x, y, w, h): (i32, i32, i32, i32)) {
    let flags = SWP_NOZORDER | SWP_NOACTIVATE;
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, h, flags) };
}

pub fn show(hwnd: HWND, visible: bool) {
    let command = if visible { SW_SHOWNOACTIVATE } else { SW_HIDE };
    let _ = unsafe { ShowWindow(hwnd, command) };
}

pub fn bounds(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut r = RECT::default();
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

pub fn dpi(hwnd: HWND) -> u32 {
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => 96,
        dpi => dpi,
    }
}

/// The primary monitor's DPI: the one [`work_area`] is on. A hidden
/// window's own DPI can lag behind a monitor being plugged or unplugged.
pub fn primary_dpi() -> u32 {
    let monitor = unsafe { MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY) };
    let (mut x, mut y) = (0, 0);
    match unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) } {
        Ok(()) if x > 0 => x,
        _ => 96,
    }
}

/// The primary monitor's desktop area, excluding the taskbar.
pub fn work_area() -> (i32, i32, i32, i32) {
    let mut r = RECT::default();
    let info = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut r).cast::<c_void>()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if info.is_err() || r.right <= r.left {
        return (0, 0, 1920, 1080);
    }
    (r.left, r.top, r.right - r.left, r.bottom - r.top)
}

fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..len.max(0) as usize])
}

/// True for the desktop itself and the taskbar, which are never "an app in front".
pub fn is_shell(hwnd: HWND) -> bool {
    matches!(
        class_name(hwnd).as_str(),
        "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd"
    )
}

/// How much of its monitor a window takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cover {
    None,
    /// Hides the desktop, but an always-on-top widget still belongs above it.
    Maximized,
    /// A game, a video, F11: nothing should be drawn over it.
    Fullscreen,
}

/// An opaque id for the monitor a window is on.
pub fn monitor(hwnd: HWND) -> isize {
    unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) }.0 as isize
}

/// Whether `hwnd` is an app covering its monitor. With an auto-hiding
/// taskbar a maximized window has the same bounds as a fullscreen one, so
/// the two are told apart by the maximized state, not by size.
pub fn cover(hwnd: HWND) -> Cover {
    if hwnd.is_invalid() || is_shell(hwnd) {
        return Cover::None;
    }
    if unsafe { IsZoomed(hwnd) }.as_bool() {
        return Cover::Maximized;
    }
    let mut window = RECT::default();
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    unsafe {
        if GetWindowRect(hwnd, &mut window).is_err()
            || !GetMonitorInfoW(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST), &mut info)
                .as_bool()
        {
            return Cover::None;
        }
    }
    let screen = info.rcMonitor;
    let fills = window.left <= screen.left
        && window.top <= screen.top
        && window.right >= screen.right
        && window.bottom >= screen.bottom;
    if fills {
        Cover::Fullscreen
    } else {
        Cover::None
    }
}

unsafe extern "system" fn find_icon_host(hwnd: HWND, lp: LPARAM) -> BOOL {
    let icons = unsafe { FindWindowExW(Some(hwnd), None, w!("SHELLDLL_DefView"), PCWSTR::null()) };
    if icons.is_ok_and(|child| !child.is_invalid()) {
        // SAFETY: lparam is the `&mut HWND` passed by `desktop_host`.
        unsafe { *(lp.0 as *mut HWND) = hwnd };
        return BOOL(0);
    }
    BOOL(1)
}

/// The top-level window holding the desktop icons: Progman, or a WorkerW
/// once the shell has split the wallpaper off.
fn desktop_host() -> Option<HWND> {
    let mut found = HWND::default();
    // Stopping the enumeration early reports as an error; the result is in `found`.
    let _ = unsafe { EnumWindows(Some(find_icon_host), LPARAM(&raw mut found as isize)) };
    (!found.is_invalid()).then_some(found)
}

/// True when Show Desktop (Win+D) has lifted the desktop above `widget`.
///
/// Show Desktop does not minimise anything: it raises the desktop's own
/// window over every other window, bottom-pinned widgets included.
pub fn desktop_covers(widget: HWND) -> bool {
    let Some(host) = desktop_host() else {
        return false;
    };
    // GW_HWNDPREV walks toward the top of the z-order.
    let mut cursor = widget;
    for _ in 0..4096 {
        match unsafe { GetWindow(cursor, GW_HWNDPREV) } {
            Ok(above) if above == host => return true,
            Ok(above) if !above.is_invalid() => cursor = above,
            _ => return false,
        }
    }
    false
}

pub fn post(hwnd: HWND, msg: u32) {
    // Fails only when the window is gone, which means the app is exiting.
    let _ = unsafe { PostMessageW(Some(hwnd), msg, WPARAM(0), LPARAM(0)) };
}
