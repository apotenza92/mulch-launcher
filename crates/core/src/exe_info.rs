//! What a program says about itself in its version information (the Details
//! tab of its Properties in Explorer).

use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
use windows::core::PCWSTR;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The program's product name, e.g. "World of Warcraft" for `Wow.exe`.
pub fn product_name(exe: &Path) -> Option<String> {
    let path: Vec<u16> = exe.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(path.as_ptr()), None, size, data.as_mut_ptr().cast()).ok()?;

        // The first language the file has strings for.
        let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
        let query = wide(r"\VarFileInfo\Translation");
        let languages = if VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool()
            && len >= 4
        {
            let pair = std::slice::from_raw_parts(ptr as *const u16, 2);
            vec![format!("{:04x}{:04x}", pair[0], pair[1])]
        } else {
            Vec::new()
        };

        // Fall back to the common US English / Unicode tables.
        languages.iter().map(String::as_str).chain(["040904b0", "040904e4", "000004b0"]).find_map(|language| {
            let query = wide(&format!(r"\StringFileInfo\{language}\ProductName"));
            let (mut ptr, mut len) = (std::ptr::null_mut(), 0u32);
            if !VerQueryValueW(data.as_ptr().cast(), PCWSTR(query.as_ptr()), &mut ptr, &mut len).as_bool() || len == 0 {
                return None;
            }
            let text = String::from_utf16_lossy(std::slice::from_raw_parts(ptr as *const u16, len as usize));
            let text = text.trim_end_matches('\0').trim().to_string();
            (!text.is_empty()).then_some(text)
        })
    }
}
