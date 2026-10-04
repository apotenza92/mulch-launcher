//! Starts games, launchers and uninstallers.

use crate::scan::Action;
use std::io;
use std::os::windows::process::CommandExt;
use std::process::Command;

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
        Action::XboxAppPage(family_name) => {
            let family_name = family_name.clone();
            // The lookup is a quick web request: off the UI thread.
            std::thread::spawn(move || {
                let link = match mulch_posters::store_product_id(&family_name) {
                    Some(id) => format!("msxbox://game/?productId={id}"),
                    None => format!("ms-windows-store://pdp/?PFN={family_name}"),
                };
                let _ = open_link(&link);
            });
            Ok(())
        }
    }
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
