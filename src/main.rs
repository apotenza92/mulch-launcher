// No console window for the app itself in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod min_width;
mod ui;

use mulch_launcher::{install, scan};

fn main() {
    let has = |flag: &str| std::env::args().any(|a| a == flag);
    if has("--scan") {
        print_scan();
        return;
    }
    if has("--discover") {
        print_discover();
        return;
    }
    // Run by Windows' "Installed apps" > Uninstall.
    if has("--uninstall") {
        let _ = install::uninstall();
        return;
    }
    // Run from anywhere else (e.g. Downloads): install silently, then hand
    // over to the installed copy. Nothing is saved next to this copy.
    if !install::is_dev_build() && !install::is_installed_copy() {
        if let Err(err) = install::install().and_then(|installed| install::relaunch(&installed)) {
            message_box(&format!("MulchLauncher couldn't install: {err}"));
        }
        return;
    }
    // Development builds keep their own data in `target`; copy in the data
    // older versions kept in AppData, leaving it there.
    if install::is_dev_build() {
        if let Some(data) = mulch_core::paths::data_dir() {
            install::adopt_legacy_data(&data, false);
        }
    }
    ui::run();
}

fn message_box(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("MulchLauncher"), MB_OK | MB_ICONERROR);
    }
}

/// `MulchLauncher --scan`: prints everything found and how long it took.
/// `--scan steam epic` scans only those libraries; `--json` prints full
/// details, including launch, uninstall and "show in launcher" commands.
fn print_scan() {
    let only: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
    let result = scan::scan_all(&only);
    if std::env::args().any(|a| a == "--json") {
        println!("{}", serde_json::to_string_pretty(&result.games).unwrap_or_default());
        return;
    }
    for game in &result.games {
        println!(
            "{:<10} {:<45} {}",
            game.platform.label(),
            game.name,
            game.install_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default()
        );
    }
    println!("\nLaunchers: {}", result.launchers.iter().map(|l| l.name).collect::<Vec<_>>().join(", "));
    println!("Chat apps: {}", result.social.iter().map(|l| l.name).collect::<Vec<_>>().join(", "));
    println!("\n{} games in {:?}", result.games.len(), result.total);
    for (name, took) in &result.timings {
        println!("  {name:<8} {took:?}");
    }
}

/// `MulchLauncher --discover`: lists programs that look like games but
/// aren't in any launcher, with the evidence for each.
fn print_discover() {
    let started = std::time::Instant::now();
    let result = scan::scan_all(&[]);
    let known = scan::known_folders(&result.games, result.launchers.iter().chain(&result.social));
    let suggestions = mulch_discover::find_games(&known);
    for s in &suggestions {
        println!("{:<34} {}\n{:<34} ({})", s.name, s.exe.display(), "", s.reason);
    }
    println!("\n{} suggestions in {:?}", suggestions.len(), started.elapsed());
}