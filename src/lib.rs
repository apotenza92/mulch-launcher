//! Everything except the window: scanning, launching and grid
//! sizing. Kept separate from the UI so it can be tested without compiling
//! the UI's (very deep) element types in test mode.

pub mod install;
pub mod launch;
pub mod layout;
pub mod restore;
pub mod scan;
pub mod settings;

/// Startup timing, for development: with `MULCH_TRACE` set, each call
/// appends "<ms since start> <what>" to `%TEMP%\mulch-trace.txt`.
pub mod trace {
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    static ON: OnceLock<bool> = OnceLock::new();

    pub fn start() {
        START.get_or_init(Instant::now);
    }

    pub fn mark(what: &str) {
        if !*ON.get_or_init(|| std::env::var_os("MULCH_TRACE").is_some()) {
            return;
        }
        let ms = START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.;
        let path = std::env::temp_dir().join("mulch-trace.txt");
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            use std::io::Write;
            let _ = writeln!(file, "{ms:8.1} {what}");
        }
    }
}
