//! A minimum window width that can change while the app runs (gpui only
//! takes one when the window opens). The toolbar sets it to whatever its
//! icons and buttons need, so they never end up off screen.

use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetWindowRect, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, WM_GETMINMAXINFO,
};

/// Minimum width of the window's content, in logical pixels.
static MIN_CLIENT_WIDTH: AtomicU32 = AtomicU32::new(0);

/// Starts enforcing the minimum width on this window. Call once.
pub fn install(hwnd: isize) {
    unsafe {
        let _ = SetWindowSubclass(HWND(hwnd as _), Some(subclass_proc), 1, 0);
    }
}

/// Sets the minimum content width, widening the window now if it's narrower.
pub fn set(hwnd: isize, width: f32) {
    let width = width.ceil() as u32;
    if MIN_CLIENT_WIDTH.swap(width, Ordering::Relaxed) == width {
        return;
    }
    let hwnd = HWND(hwnd as _);
    unsafe {
        let (window, client) = (rect(hwnd, false), rect(hwnd, true));
        let needed = to_device(hwnd, width) + frame_width(window, client);
        if window.right - window.left < needed {
            let height = window.bottom - window.top;
            let _ = SetWindowPos(hwnd, None, 0, 0, needed, height, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }
}

unsafe extern "system" fn subclass_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // gpui fills in its own minimum first; then raise the width if needed.
    let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
    if msg == WM_GETMINMAXINFO {
        let width = MIN_CLIENT_WIDTH.load(Ordering::Relaxed);
        if width > 0 {
            unsafe {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let needed = to_device(hwnd, width) + frame_width(rect(hwnd, false), rect(hwnd, true));
                info.ptMinTrackSize.x = info.ptMinTrackSize.x.max(needed);
            }
        }
    }
    result
}

unsafe fn rect(hwnd: HWND, client: bool) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = if client { GetClientRect(hwnd, &mut rect) } else { GetWindowRect(hwnd, &mut rect) };
    }
    rect
}

/// Width of the window's frame (zero or close to it with our own title bar).
fn frame_width(window: RECT, client: RECT) -> i32 {
    ((window.right - window.left) - (client.right - client.left)).max(0)
}

fn to_device(hwnd: HWND, logical: u32) -> i32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    (logical as f32 * dpi as f32 / 96.).ceil() as i32
}
