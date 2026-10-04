//! Window sizing gpui can't do on its own:
//! - A minimum width that changes while the app runs: whatever the toolbar's
//!   icons and buttons need, so they never end up off screen.
//! - Widths that snap, while the user drags an edge, to exactly fit whole
//!   columns of tiles, so the grid's margins are always equal. (Maximised or
//!   screen-snapped windows aren't snapped; the grid is centred instead.)
//!
//! All sizes here are logical pixels, converted with the window's DPI.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, IsZoomed, MINMAXINFO, PostMessageW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
    SetWindowPos, WM_APP, WM_GETMINMAXINFO, WM_SIZING, WMSZ_BOTTOMLEFT, WMSZ_LEFT, WMSZ_TOPLEFT,
};

/// Posted to ourselves to snap the width after the current frame: resizing
/// the window from inside gpui's own drawing is ignored.
const SNAP_NOW: u32 = WM_APP + 0x4D;

/// Minimum width of the window's content.
static MIN_WIDTH: AtomicU32 = AtomicU32::new(0);
/// Snapping: content width = `SNAP_BASE` + n × `SNAP_STEP` − `SNAP_GAP`, for
/// n whole columns (stored as f32 bits; 0 = no snapping).
static SNAP_BASE: AtomicU32 = AtomicU32::new(0);
static SNAP_STEP: AtomicU32 = AtomicU32::new(0);
static SNAP_GAP: AtomicU32 = AtomicU32::new(0);

/// Starts managing this window's width. Call once.
pub fn install(hwnd: isize) {
    unsafe {
        let _ = SetWindowSubclass(HWND(hwnd as _), Some(subclass_proc), 1, 0);
    }
}

/// Sets the minimum content width, widening the window now if it's narrower.
pub fn set_min(hwnd: isize, width: f32) {
    let width = width.ceil() as u32;
    if MIN_WIDTH.swap(width, Ordering::Relaxed) == width {
        return;
    }
    let hwnd = HWND(hwnd as _);
    let current = content_width(hwnd);
    if current < width as f32 {
        resize_content(hwnd, snapped(width as f32));
    }
}

/// Sets what widths fit whole columns: `base` (margins), plus columns of
/// `step` (tile + gap), less one `gap`. With `snap_now`, the window moves to
/// the nearest such width immediately (e.g. after zooming).
pub fn set_snap(hwnd: isize, base: f32, step: f32, gap: f32, snap_now: bool) {
    let swap = |value: &AtomicU32, new: f32| value.swap(new.to_bits(), Ordering::Relaxed) != new.to_bits();
    let changed = [swap(&SNAP_BASE, base), swap(&SNAP_STEP, step), swap(&SNAP_GAP, gap)].contains(&true);
    if changed && snap_now {
        unsafe {
            let _ = PostMessageW(Some(HWND(hwnd as _)), SNAP_NOW, WPARAM(0), LPARAM(0));
        }
    }
}

/// The nearest width that fits whole columns, and at least the minimum.
fn snapped(width: f32) -> f32 {
    let base = f32::from_bits(SNAP_BASE.load(Ordering::Relaxed));
    let step = f32::from_bits(SNAP_STEP.load(Ordering::Relaxed));
    let gap = f32::from_bits(SNAP_GAP.load(Ordering::Relaxed));
    let min = MIN_WIDTH.load(Ordering::Relaxed) as f32;
    if step <= 0. {
        return width.max(min);
    }
    let fit = |columns: f32| base + columns * step - gap;
    let mut columns = ((width - base + gap) / step).round().max(1.);
    while fit(columns) < min {
        columns += 1.;
    }
    fit(columns)
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // gpui handles the message first; then we adjust its answer.
    let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    match msg {
        SNAP_NOW => {
            if !unsafe { IsZoomed(hwnd) }.as_bool() {
                resize_content(hwnd, snapped(content_width(hwnd)));
            }
        }
        WM_GETMINMAXINFO => {
            let min = MIN_WIDTH.load(Ordering::Relaxed);
            if min > 0 {
                let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
                let needed = to_device(hwnd, snapped(min as f32)) + frame_width(hwnd);
                info.ptMinTrackSize.x = info.ptMinTrackSize.x.max(needed);
            }
        }
        // The user is dragging an edge: move the side being dragged so the
        // content is a whole number of columns wide.
        WM_SIZING => {
            let rect = unsafe { &mut *(lparam.0 as *mut RECT) };
            let frame = frame_width(hwnd);
            let content = to_logical(hwnd, rect.right - rect.left - frame);
            let width = to_device(hwnd, snapped(content)) + frame;
            let edge = wparam.0 as u32;
            if matches!(edge, WMSZ_LEFT | WMSZ_TOPLEFT | WMSZ_BOTTOMLEFT) {
                rect.left = rect.right - width;
            } else {
                rect.right = rect.left + width;
            }
            return LRESULT(1);
        }
        _ => {}
    }
    result
}

fn rect(hwnd: HWND, client: bool) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = if client { GetClientRect(hwnd, &mut rect) } else { GetWindowRect(hwnd, &mut rect) };
    }
    rect
}

/// Width of the window's frame (zero or close to it with our own title bar).
fn frame_width(hwnd: HWND) -> i32 {
    let (window, client) = (rect(hwnd, false), rect(hwnd, true));
    ((window.right - window.left) - (client.right - client.left)).max(0)
}

fn content_width(hwnd: HWND) -> f32 {
    let client = rect(hwnd, true);
    to_logical(hwnd, client.right - client.left)
}

fn resize_content(hwnd: HWND, width: f32) {
    let window = rect(hwnd, false);
    let width = to_device(hwnd, width) + frame_width(hwnd);
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            width,
            window.bottom - window.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn scale(hwnd: HWND) -> f32 {
    unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.
}

fn to_device(hwnd: HWND, logical: f32) -> i32 {
    (logical * scale(hwnd)).round() as i32
}

fn to_logical(hwnd: HWND, device: i32) -> f32 {
    device as f32 / scale(hwnd)
}
