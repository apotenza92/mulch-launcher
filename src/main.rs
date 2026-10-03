// No console window for the app itself in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;

use mulch_launcher::{install, scan};

fn main() {
    let has = |flag: &str| std::env::args().any(|a| a == flag);
    if has("--scan") {
        print_scan();
        return;
    }
    // Run by Windows' "Installed apps" > Uninstall.
    if has("--uninstall") {
        if let Err(err) = install::uninstall() {
            eprintln!("Uninstall failed: {err}");
        }
        return;
    }
    // Passed by setup when the user asked to be pinned to the taskbar.
    ui::run(has("--pin"));
}

/// `MulchLauncher --scan`: prints everything found and how long it took.
fn print_scan() {
    let result = scan::scan_all();
    for game in &result.games {
        println!(
            "{:<10} {:<45} {}",
            game.platform.label(),
            game.name,
            game.install_dir.as_ref().map(|d| d.display().to_string()).unwrap_or_default()
        );
    }
    println!("\nLaunchers: {}", result.launchers.iter().map(|l| l.name).collect::<Vec<_>>().join(", "));
    println!("\n{} games in {:?}", result.games.len(), result.total);
    for (name, took) in &result.timings {
        println!("  {name:<8} {took:?}");
    }
}
