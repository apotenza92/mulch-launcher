//! Starts games, launchers and uninstallers.

use crate::scan::Action;
use std::io;
use std::process::Command;

pub fn run(action: &Action) -> io::Result<()> {
    match action {
        // `explorer.exe <uri>` hands the URI to whichever launcher registered it.
        Action::Uri(uri) => Command::new("explorer.exe").arg(uri).spawn().map(|_| ()),
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
    }
}
