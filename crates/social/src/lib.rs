//! Chat apps people use to get a game night together (Discord, ...), shown
//! as buttons at the start of the toolbar when installed.
//!
//! Each is found the way it registers itself: a link handler, a Microsoft
//! Store package, or a Windows "Installed apps" entry. Never a default path.

use mulch_core::registry;
use mulch_core::{Launcher, ScanContext};

struct ChatApp {
    name: &'static str,
    /// Link handlers it registers, e.g. `discord://`.
    schemes: &'static [&'static str],
    /// Microsoft Store package family names.
    store_packages: &'static [&'static str],
    /// Prefix of its Windows uninstall entry name.
    uninstall_name: Option<&'static str>,
}

const CHAT_APPS: &[ChatApp] = &[
    ChatApp { name: "Discord", schemes: &["discord"], store_packages: &[], uninstall_name: Some("Discord") },
    ChatApp {
        name: "Messenger",
        schemes: &[],
        store_packages: &["FACEBOOK.317180B0BB486_8xx8rvfyw5nnt"],
        uninstall_name: Some("Messenger"),
    },
    ChatApp {
        name: "Telegram",
        schemes: &["tg"],
        store_packages: &["TelegramMessengerLLP.TelegramDesktop_t4vj0pshhgkwm"],
        uninstall_name: Some("Telegram Desktop"),
    },
    ChatApp { name: "Signal", schemes: &["sgnl"], store_packages: &[], uninstall_name: Some("Signal") },
    ChatApp {
        name: "TeamSpeak",
        schemes: &["teamspeak", "ts3server"],
        store_packages: &[],
        uninstall_name: Some("TeamSpeak"),
    },
    ChatApp { name: "Mumble", schemes: &["mumble"], store_packages: &[], uninstall_name: Some("Mumble") },
];

/// Installed chat apps, alphabetically.
pub fn detect(cx: &ScanContext) -> Vec<Launcher> {
    let mut found: Vec<Launcher> = CHAT_APPS
        .iter()
        .filter_map(|app| {
            app.store_packages.iter().find_map(|package| mulch_xbox::store_app(package, app.name)).or_else(|| {
                let exe = registry::launcher_exe(cx.uninstall_entries(), app.schemes, app.uninstall_name)?;
                let mut launcher = Launcher::from_exe(mulch_core::Platform::Manual, app.name, exe);
                launcher.platform = None;
                Some(launcher)
            })
        })
        .collect();
    found.sort_by_key(|l| l.name.to_lowercase());
    found
}
