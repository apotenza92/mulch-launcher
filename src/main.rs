// No console window for the app itself in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod assets;
mod ui;
mod window_size;

use mulch_launcher::{install, scan};

fn main() {
    mulch_launcher::trace::start();
    let has = |flag: &str| std::env::args().any(|a| a == flag);
    if has("--scan") {
        print_scan();
        return;
    }
    // Development aid: write every launcher's glyph into a folder.
    if let Some(ix) = std::env::args().position(|a| a == "--glyphs") {
        let dir = std::env::args().nth(ix + 1).unwrap_or_else(|| ".".into());
        let mut launchers = scan::scan_all(&[]).launchers;
        mulch_art::fill_launchers(&mut launchers);
        for l in &launchers {
            if let Some(icon) = &l.icon {
                for (ink, tag) in [([255, 255, 255], "white"), ([30, 30, 30], "dark")] {
                    if let Some(g) = mulch_art::glyph(icon, ink) {
                        let _ = std::fs::copy(&g, format!("{dir}/{}-{tag}.png", l.name));
                    }
                }
                let _ = std::fs::copy(icon, format!("{dir}/{}-icon.png", l.name));
            }
        }
        return;
    }
    if has("--discover") {
        print_discover();
        return;
    }
    // Store installs open directly into the library; add the desktop link once.
    if mulch_core::paths::is_packaged() {
        std::thread::spawn(|| {
            if let Err(error) = install::add_store_desktop_shortcut() {
                mulch_launcher::trace::mark(&format!("desktop shortcut: {error}"));
            }
        });
    }
    // Development builds keep their own data in `target`; copy in the data
    // older versions kept in AppData, leaving it there.
    if install::is_dev_build() {
        if let Some(data) = mulch_core::paths::data_dir() {
            install::adopt_legacy_data(&data, false);
        }
    }
    mulch_launcher::trace::mark("main done");
    ui::run();
}

/// `MulchLauncher --scan`: prints everything found and how long it took.
/// `--scan steam epic` scans only those libraries; `--json` prints full
/// details, including launch, uninstall and "show in launcher" commands.
fn print_scan() {
    let only: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
    let mut result = scan::scan_all(&only);
    // Repeat scans show the cost of the app's periodic re-checks.
    if std::env::args().any(|a| a == "--repeat") {
        for _ in 0..5 {
            result = scan::scan_all(&only);
            println!("again: {:?}", result.total);
        }
    }
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
