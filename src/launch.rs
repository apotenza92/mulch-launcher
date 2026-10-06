//! Starts games, launchers and uninstallers.

use crate::scan::Action;
use std::collections::HashMap;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::{LazyLock, Mutex};

/// Don't flash a console window when going through `cmd`.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn run(action: &Action) -> io::Result<()> {
    match action {
        // Handed to whichever app registered the link (not explorer.exe <uri>,
        // which misreads links with query strings and opens a folder instead).
        Action::Uri(uri) => open_link(uri),
        Action::StoreApp(app_id) => {
            Command::new("explorer.exe").arg(format!(r"shell:AppsFolder\{app_id}")).spawn().map(|_| ())
        }
        Action::Exe { path, args, working_dir } => {
            let mut command = Command::new(path);
            command.args(args);
            if let Some(dir) = working_dir.as_ref().or(path.parent().map(|p| p.to_path_buf()).as_ref()) {
                command.current_dir(dir);
            }
            command.spawn().map(|_| ())
        }
        // `/S /C "<line>"` makes cmd strip exactly the outer quotes we add and
        // run the line verbatim, nested quotes and all.
        Action::CommandLine(line) => Command::new("cmd")
            .raw_arg(format!("/S /C \"{line}\""))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ()),
        // The game's page in the Xbox app; its Store page if the id can't be found.
        // Usually looked up already (see look_up_xbox_pages), so it opens at once.
        Action::XboxAppPage(family_name) => {
            if let Some(id) = XBOX_PRODUCT_IDS.lock().unwrap().get(family_name).cloned() {
                return open_link(&xbox_app_link(&id));
            }
            let family_name = family_name.clone();
            // Not looked up yet: a quick web request, off the UI thread.
            std::thread::spawn(move || {
                let link = match look_up_xbox_page(&family_name) {
                    Some(id) => xbox_app_link(&id),
                    None => format!("ms-windows-store://pdp/?PFN={family_name}"),
                };
                let _ = open_link(&link);
            });
            Ok(())
        }
    }
}

/// Each Xbox game's Store product id, by package family name: looked up once,
/// so its Show in Xbox app link needs no web request when it's clicked.
static XBOX_PRODUCT_IDS: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);

/// Looks up (in the background) the Xbox app pages of these package families
/// that aren't known yet.
pub fn look_up_xbox_pages(family_names: Vec<String>) {
    let missing: Vec<String> = {
        let known = XBOX_PRODUCT_IDS.lock().unwrap();
        family_names.into_iter().filter(|name| !known.contains_key(name)).collect()
    };
    if missing.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for name in missing {
            look_up_xbox_page(&name);
        }
    });
}

fn look_up_xbox_page(family_name: &str) -> Option<String> {
    let id = mulch_posters::store_product_id(family_name)?;
    XBOX_PRODUCT_IDS.lock().unwrap().insert(family_name.to_string(), id.clone());
    Some(id)
}

fn xbox_app_link(product_id: &str) -> String {
    format!("msxbox://game/?productId={product_id}")
}

/// Opens a link with the app registered for it, as if it were clicked.
fn open_link(link: &str) -> io::Result<()> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::{HSTRING, w};
    let result = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(link), None, None, SW_SHOWNORMAL) };
    // Values above 32 mean it worked.
    if result.0 as isize > 32 { Ok(()) } else { Err(io::Error::other(format!("couldn't open {link}"))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Battle.net uninstall strings look like
    /// `"C:\...\Blizzard Uninstaller.exe" --uid=wow --displayname="World of Warcraft"`;
    /// the quoted, spaced parts must reach the program untouched.
    #[test]
    fn command_lines_run_verbatim() {
        let out = std::env::temp_dir().join(format!("mulch-cmdline-test-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let line =
            format!(r#""C:\Windows\System32\cmd.exe" /c echo --displayname="World of Warcraft"> "{}""#, out.display());
        run(&Action::CommandLine(line)).unwrap();

        let started = Instant::now();
        let text = loop {
            if let Ok(text) = std::fs::read_to_string(&out) {
                if !text.is_empty() {
                    break text;
                }
            }
            assert!(started.elapsed() < Duration::from_secs(5), "command never ran");
            std::thread::sleep(Duration::from_millis(50));
        };
        let _ = std::fs::remove_file(&out);
        assert_eq!(text.trim(), r#"--displayname="World of Warcraft""#);
    }
}
