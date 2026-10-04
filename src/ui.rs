//! The main window: a slim title bar, a toolbar (chat apps and launchers on
//! the left, buttons on the right) and every detected game as a tile, grouped
//! by when it was last played and sized so they all fit if they can.

use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{WindowExt, *};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use mulch_art as art;
use mulch_discover::Suggestion;
use mulch_history::{self as history, History};
use mulch_launcher::install;
use mulch_launcher::launch;
use mulch_launcher::layout::{
    COVER_ASPECT, GRID_GAP, GridLayout, HEADING_HEIGHT, LABEL_HEIGHT, TILE_WIDTH, grid_width, layout as grid_layout,
};
use mulch_launcher::scan::{self, Action, Art, Game, Launcher, Platform, ScanResult};
use mulch_launcher::settings::{Settings, ThemeChoice};
use mulch_manual as manual;
use mulch_posters as posters;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

const APP_NAME: &str = "MulchLauncher";

pub fn run() {
    gpui_kit::application().with_assets(crate::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(theme_mode(Settings::load().theme, cx.window_appearance()), None, cx);
        make_glassy(cx);

        let options = WindowOptions {
            // Our own slim title bar (see `title_bar`), with the toolbar under
            // it. The title is still set for the taskbar and Alt+Tab.
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            // Frosted glass: the desktop behind shows through, blurred.
            window_background: WindowBackgroundAppearance::Blurred,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, default_window_size(cx), cx))),
            // The width also grows to fit the toolbar (see `update_min_width`).
            window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(360.))),
            app_id: Some(APP_NAME.into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| MulchApp::new(window, cx)))
            .expect("failed to open the window");
    });
}

/// The one window shown when the downloaded copy is run: an "Add to desktop"
/// checkbox (on by default) and Install. Closing it installs nothing.
pub fn run_installer() {
    gpui_kit::application().with_assets(crate::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(theme_mode(ThemeChoice::System, cx.window_appearance()), None, cx);
        make_glassy(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            // Frosted glass: the desktop behind shows through, blurred.
            window_background: WindowBackgroundAppearance::Blurred,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(INSTALLER_WIDTH), px(INSTALLER_HEIGHT)),
                cx,
            ))),
            is_resizable: false,
            app_id: Some(APP_NAME.into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| Installer::new(window, cx)))
            .expect("failed to open the window");
    });
}

const INSTALLER_WIDTH: f32 = 440.;
const INSTALLER_HEIGHT: f32 = 232.;
/// Room for the "also add these games" heading, and for each game found.
const INSTALLER_LIST_HEADING: f32 = 40.;
const INSTALLER_LIST_ROW: f32 = 50.;
/// Games shown before the list scrolls.
const INSTALLER_LIST_ROWS: usize = 6;

/// The install window: "Add to desktop", games found on this PC that no
/// launcher knows about (ticked, to add along with installing), and Install.
struct Installer {
    desktop: bool,
    /// None while still looking.
    found: Option<Vec<Suggestion>>,
    selected: HashSet<PathBuf>,
    error: Option<String>,
}

impl Installer {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Look for other games while the user decides; grow to list them.
        cx.spawn_in(window, async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    let result = scan::scan_all(&[]);
                    let known = scan::known_folders(&result.games, result.launchers.iter().chain(&result.social));
                    mulch_discover::find_games(&known)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                if !found.is_empty() {
                    let rows = found.len().min(INSTALLER_LIST_ROWS) as f32;
                    let height = INSTALLER_HEIGHT + INSTALLER_LIST_HEADING + rows * INSTALLER_LIST_ROW;
                    window.resize(size(px(INSTALLER_WIDTH), px(height)));
                }
                this.selected = found.iter().map(|s| s.exe.clone()).collect();
                this.found = Some(found);
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self { desktop: true, found: None, selected: HashSet::new(), error: None }
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        let chosen: Vec<(PathBuf, String)> = self
            .found
            .iter()
            .flatten()
            .filter(|s| self.selected.contains(&s.exe))
            .map(|s| (s.exe.clone(), s.name.clone()))
            .collect();
        let result = std::env::current_exe().and_then(|download| {
            let installed = install::install(self.desktop)?;
            if let (false, Some(dir)) = (chosen.is_empty(), installed.parent()) {
                manual::add_all_in(&dir.join("data"), chosen)?;
            }
            install::hand_over(&installed, &download)
        });
        match result {
            Ok(()) => cx.quit(),
            Err(err) => {
                self.error = Some(format!("Couldn't install: {err}"));
                cx.notify();
            }
        }
    }
}

impl Render for Installer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let found = match &self.found {
            None => Some(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Looking for games that aren't in Steam, Epic or other launchers…")
                    .into_any_element(),
            ),
            Some(found) if found.is_empty() => None,
            Some(found) => Some(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Found these games that aren't in any launcher. Add them too?"),
                    )
                    .child(
                        v_flex()
                            .id("found")
                            // Room for the rows' hover background, which reaches past their content.
                            .mx(px(-8.))
                            .px(px(8.))
                            .max_h(px(INSTALLER_LIST_ROWS as f32 * INSTALLER_LIST_ROW))
                            .overflow_y_scroll()
                            .children(found.iter().enumerate().map(|(ix, game)| {
                                let exe = game.exe.clone();
                                let detail = game.exe.display().to_string().into();
                                check_row(
                                    ("found", ix),
                                    self.selected.contains(&game.exe),
                                    game.name.clone(),
                                    Some(detail),
                                    cx,
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if !this.selected.remove(&exe) {
                                        this.selected.insert(exe.clone());
                                    }
                                    cx.notify();
                                }))
                            })),
                    )
                    .into_any_element(),
            ),
        };
        v_flex()
            .size_full()
            .bg(theme.background)
            .child(TitleBar::new().bg(gpui_kit::transparent_black()).border_color(gpui_kit::transparent_black()))
            .child(
                v_flex()
                    .flex_1()
                    .px_6()
                    .pb_6()
                    .gap_4()
                    .child(div().text_xl().font_semibold().child(format!("Install {APP_NAME}")))
                    .child(check_row("desktop", self.desktop, "Add to desktop", None, cx).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.desktop = !this.desktop;
                            cx.notify();
                        },
                    )))
                    .children(found)
                    .children(self.error.clone().map(|error| div().text_sm().text_color(theme.danger).child(error)))
                    .child(div().flex_1())
                    .child(
                        h_flex().justify_end().child(
                            Button::new("install")
                                .primary()
                                .label("Install")
                                .on_click(cx.listener(|this, _, _, cx| this.install(cx))),
                        ),
                    ),
            )
    }
}
struct MulchApp {
    games: Vec<Game>,
    launchers: Vec<Launcher>,
    /// Installed chat apps (Discord, ...).
    social: Vec<Launcher>,
    /// "Add game manually", while it's showing.
    add_panel: Option<AddPanel>,
    /// Natural widths of the toolbar's left and right groups, measured as
    /// they're laid out, which set the window's minimum width.
    toolbar_widths: Rc<[Cell<f32>; 2]>,
    /// The native window, once known.
    hwnd: Option<isize>,
    /// The game tile under the mouse, which shows its full name.
    hovered_tile: Option<usize>,
    /// Where the mouse is over the hovered poster, from -1 to 1 across and
    /// down (0, 0 is the middle), for its tilt effect.
    tilt: (f32, f32),
    /// The hovered poster's bounds, measured as it's laid out.
    hovered_bounds: Rc<Cell<Bounds<Pixels>>>,
    scanning: bool,
    /// The added game whose Remove button was clicked once (the next click removes).
    confirm_remove: Option<String>,
    /// Each game's hover lift (0 resting, 1 lifted), eased over time.
    lifts: HashMap<String, Tween>,
    /// Games being removed, fading out before they go.
    leaving: HashMap<String, Tween>,
    /// Each poster's accent colour, for tinting its glow.
    accents: HashMap<PathBuf, Hsla>,
    /// A theme change in progress: the colours it's blending from and to.
    theme_fade: Option<(ThemeColor, ThemeColor, Tween)>,
    /// When games first appeared: until shortly after, tiles enter one after
    /// another; later additions just fade straight in.
    first_shown: Option<std::time::Instant>,
    /// Set by an action's click, so the tile under it doesn't also react.
    action_clicked: bool,
    /// When each game was last played, for sorting most recent first.
    history: History,
    settings: Settings,
    /// The theme button's tooltip text, shared with the tooltip so it updates on click.
    theme_tip: Rc<Cell<&'static str>>,
    _subscriptions: Vec<Subscription>,
}

/// How often to look for running games (to record them as played).
const PLAY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Programs on this PC that look like games, to add with a tick.
struct AddPanel {
    /// None while still looking.
    suggestions: Option<Vec<Suggestion>>,
    selected: HashSet<PathBuf>,
}

impl MulchApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = Self {
            games: Vec::new(),
            launchers: Vec::new(),
            social: Vec::new(),
            add_panel: None,
            toolbar_widths: Rc::default(),
            hwnd: None,
            hovered_tile: None,
            tilt: (0., 0.),
            hovered_bounds: Rc::default(),
            scanning: false,
            confirm_remove: None,
            lifts: HashMap::new(),
            leaving: HashMap::new(),
            accents: HashMap::new(),
            theme_fade: None,
            first_shown: None,
            action_clicked: false,
            history: History::load(),
            // Coming back to the window (e.g. after playing): re-sort so the
            // game just played is first.
            settings: Settings::load(),
            theme_tip: Rc::default(),
            _subscriptions: vec![
                // Follow Windows switching between light and dark, when set to.
                cx.observe_window_appearance(window, |app, window, cx| {
                    if app.settings.theme == ThemeChoice::System {
                        app.fade_theme(window, cx);
                    }
                }),
                cx.observe_window_activation(window, |app, window, cx| {
                    if window.is_window_active() {
                        app.resort(cx);
                    }
                }),
            ],
        };
        app.apply_theme(window, cx);
        app.rescan(cx);
        app.watch_for_running_games(cx);
        app
    }

    /// Every so often, records any game with a process running from its
    /// folder as played. This catches games started from their own launcher
    /// too, on every platform.
    fn watch_for_running_games(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PLAY_CHECK_INTERVAL).await;
                let Ok(games) = this.update(cx, |app, _| app.games.clone()) else { break };
                let running = cx.background_spawn(async move { history::running_games(&games) }).await;
                if running.is_empty() {
                    continue;
                }
                let updated = this.update(cx, |app, _| {
                    let now = history::now();
                    for id in &running {
                        app.history.record(id, now);
                    }
                    app.history.save();
                });
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Re-sorts most recently played first (never-played games A-Z).
    fn resort(&mut self, cx: &mut Context<Self>) {
        self.history.refresh_windows_record();
        self.history.sort(&mut self.games);
        cx.notify();
    }

    /// Scans on a background thread so the window opens instantly, then
    /// fills in icons (slower on first run) as a second step.
    fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { scan::scan_all(&[]) }).await;
            let (mut games, mut launchers) = (result.games.clone(), result.launchers.clone());
            launchers.extend(result.social.iter().cloned());
            this.update(cx, |app, cx| app.apply(result, cx)).ok();

            let (mut games, launchers) = cx
                .background_spawn(async move {
                    art::fill_missing(&mut games);
                    art::fill_launchers(&mut launchers);
                    (games, launchers)
                })
                .await;
            this.update(cx, |app, cx| app.apply_art(games.clone(), launchers.clone(), cx)).ok();

            // Posters for games whose launcher keeps none on disk: cached after
            // the first run, fetched online otherwise, so they arrive last.
            let games = cx
                .background_spawn(async move {
                    posters::fill_missing(&mut games);
                    games
                })
                .await;
            this.update(cx, |app, cx| app.apply_art(games, launchers, cx)).ok();
        })
        .detach();
    }

    fn apply(&mut self, result: ScanResult, cx: &mut Context<Self>) {
        // Keep the art already showing (icons and downloaded posters arrive
        // after the scan), so a rescan doesn't blank every tile for a moment.
        let mut shown: HashMap<String, Art> = self.games.drain(..).filter_map(|g| Some((g.id, g.art?))).collect();
        self.games = result.games;
        if self.first_shown.is_none() && !self.games.is_empty() {
            self.first_shown = Some(std::time::Instant::now());
        }
        for game in &mut self.games {
            if !matches!(game.art, Some(Art::Cover(_))) {
                if let Some(art) = shown.remove(&game.id) {
                    game.art = Some(art);
                }
            }
        }
        self.history.sort(&mut self.games);
        self.launchers = result.launchers;
        // Launchers with the most games first; ties alphabetical.
        let games_on =
            |launcher: &Launcher| self.games.iter().filter(|g| Some(g.platform) == launcher.platform).count();
        let mut launchers = std::mem::take(&mut self.launchers);
        launchers.sort_by_cached_key(|l| (std::cmp::Reverse(games_on(l)), l.name.to_lowercase()));
        self.launchers = launchers;
        self.social = result.social;
        self.scanning = false;
        self.confirm_remove = None;
        cx.notify();
    }

    fn play(&mut self, game: &Game, window: &mut Window, cx: &mut Context<Self>) {
        run_action(&game.launch, &game.name, window, cx);
        self.history.record(&game.id, history::now());
        self.history.save();
        cx.notify();
    }

    /// Glass buttons over a hovered poster: Play follows the cursor (staying
    /// clear of the bottom row); along the bottom, Show in folder on the left
    /// and the game's other action on the right: Show in its launcher, or for
    /// games the user added, Remove (which asks again first).
    fn poster_actions(&self, game: &Game, size: (f32, f32), cursor: (f32, f32), cx: &mut Context<Self>) -> Div {
        let mut left: Option<AnyElement> = None;
        let mut right: Option<AnyElement> = None;
        if let Some(dir) = game.install_dir.clone() {
            let button = glass_button("show-folder", IconName::FolderOpen, GLASS_BUTTON, false).on_click(cx.listener(
                move |app, _, _, cx| {
                    app.action_clicked = true;
                    cx.open_with_system(&dir);
                },
            ));
            left = Some(with_tooltip(button, "Show in folder").into_any_element());
        }
        if let Some(show) = game.show_in_launcher.clone() {
            let launcher = match game.platform {
                Platform::Xbox => "Microsoft Store",
                Platform::Gog => "GOG Galaxy",
                other => other.label(),
            };
            let name = game.name.clone();
            let button = glass_button("show-launcher", IconName::ExternalLink, GLASS_BUTTON, false).on_click(
                cx.listener(move |app, _, window, cx| {
                    app.action_clicked = true;
                    run_action(&show, &name, window, cx);
                }),
            );
            right = Some(with_tooltip(button, format!("Show in {launcher}")).into_any_element());
        } else if let (Platform::Manual, Action::Exe { path, .. }) = (game.platform, &game.launch) {
            let exe = path.clone();
            let id = game.id.clone();
            let confirming = self.confirm_remove.as_deref() == Some(game.id.as_str());
            let button = glass_button("remove", IconName::Close, GLASS_BUTTON, confirming).on_click(cx.listener(
                move |app, _, _, cx| {
                    app.action_clicked = true;
                    if app.confirm_remove.as_deref() == Some(id.as_str()) {
                        app.confirm_remove = None;
                        app.remove_manual(exe.clone(), cx);
                    } else {
                        app.confirm_remove = Some(id.clone());
                        cx.notify();
                    }
                },
            ));
            let tip = if confirming { "Click again to remove" } else { "Remove" };
            right = Some(with_tooltip(button, tip).into_any_element());
        }

        // Play sits under the cursor, kept inside the poster and above the bottom row.
        let (width, height) = size;
        let half = GLASS_PLAY_BUTTON / 2.;
        let lowest = height - GLASS_INSET - GLASS_BUTTON - GLASS_GAP - half;
        let x = cursor.0.clamp(GLASS_INSET + half, width - GLASS_INSET - half);
        let y = cursor.1.clamp(GLASS_INSET + half, lowest.max(GLASS_INSET + half));
        let g = game.clone();
        let play = glass_button("play", IconName::Play, GLASS_PLAY_BUTTON, false).on_click(cx.listener(
            move |app, _, window, cx| {
                app.action_clicked = true;
                app.play(&g, window, cx);
            },
        ));
        let play = with_tooltip(play, "Play").absolute().left(px(x - half)).top(px(y - half));

        div().absolute().top_0().left_0().size_full().child(play).child(
            h_flex()
                .absolute()
                .left(px(GLASS_INSET))
                .right(px(GLASS_INSET))
                .bottom(px(GLASS_INSET))
                .justify_between()
                .child(left.unwrap_or_else(|| div().into_any_element()))
                .child(right.unwrap_or_else(|| div().into_any_element())),
        )
    }
    fn apply_art(&mut self, games: Vec<Game>, launchers: Vec<Launcher>, cx: &mut Context<Self>) {
        // Icons may have been added or replaced with trimmed copies.
        let game_art: HashMap<String, Art> = games.into_iter().filter_map(|g| Some((g.id, g.art?))).collect();
        for game in &mut self.games {
            if let Some(art) = game_art.get(&game.id) {
                game.art = Some(art.clone());
            }
        }
        let launcher_icons: HashMap<&str, PathBuf> =
            launchers.into_iter().filter_map(|l| Some((l.name, l.icon?))).collect();
        for launcher in self.launchers.iter_mut().chain(&mut self.social) {
            if let Some(icon) = launcher_icons.get(launcher.name) {
                launcher.icon = Some(icon.clone());
            }
        }
        self.find_accents(cx);
        cx.notify();
    }

    /// Works out the accent colour of posters that don't have one yet, in the background.
    fn find_accents(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<PathBuf> = self
            .games
            .iter()
            .filter_map(|g| match &g.art {
                Some(Art::Cover(path) | Art::Icon(path)) => Some(path.clone()),
                None => None,
            })
            .filter(|path| !self.accents.contains_key(path))
            .collect();
        if wanted.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    wanted
                        .into_iter()
                        .filter_map(|path| {
                            let [r, g, b] = art::accent(&path)?;
                            Some((
                                path,
                                Hsla::from(Rgba { r: r as f32 / 255., g: g as f32 / 255., b: b as f32 / 255., a: 1. }),
                            ))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |app, cx| {
                app.accents.extend(found);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens "Add game manually" and starts looking for games on this PC
    /// that no launcher knows about.
    fn open_add_panel(&mut self, cx: &mut Context<Self>) {
        self.add_panel = Some(AddPanel { suggestions: None, selected: HashSet::new() });
        cx.notify();
        let known = scan::known_folders(&self.games, self.launchers.iter().chain(&self.social));
        cx.spawn(async move |this, cx| {
            let found = cx.background_spawn(async move { mulch_discover::find_games(&known) }).await;
            this.update(cx, |app, cx| {
                if let Some(panel) = &mut app.add_panel {
                    panel.selected = found.iter().map(|s| s.exe.clone()).collect();
                    panel.suggestions = Some(found);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn close_add_panel(&mut self, cx: &mut Context<Self>) {
        self.add_panel = None;
        cx.notify();
    }

    fn toggle_suggestion(&mut self, exe: PathBuf, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.add_panel {
            if !panel.selected.remove(&exe) {
                panel.selected.insert(exe);
            }
            cx.notify();
        }
    }

    fn add_selected(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.add_panel.take() else { return };
        for suggestion in panel.suggestions.unwrap_or_default() {
            if panel.selected.contains(&suggestion.exe) {
                let _ = manual::add_named(&suggestion.exe, suggestion.name);
            }
        }
        self.rescan(cx);
    }

    /// Picks executables with the file picker.
    fn browse_for_games(&mut self, cx: &mut Context<Self>) {
        self.add_panel = None;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add game".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else { return };
            for path in paths {
                let _ = manual::add(&path);
            }
            this.update(cx, |app, cx| app.rescan(cx)).ok();
        })
        .detach();
    }

    /// Fades the game out, then removes it.
    fn remove_manual(&mut self, exe: PathBuf, cx: &mut Context<Self>) {
        let id = self
            .games
            .iter()
            .find(|g| matches!(&g.launch, Action::Exe { path, .. } if *path == exe))
            .map(|g| g.id.clone());
        if let Some(id) = &id {
            Tween::go(&mut self.leaving, id, 1., LEAVE_DURATION);
            cx.notify();
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LEAVE_DURATION).await;
            let _ = manual::remove(&exe);
            this.update(cx, |app, cx| {
                if let Some(id) = id {
                    app.games.retain(|g| g.id != id);
                    app.leaving.remove(&id);
                }
                app.rescan(cx);
            })
            .ok();
        })
        .detach();
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        // Clear, like the rest of the glass, and no line under it.
        TitleBar::new()
            .bg(gpui_kit::transparent_black())
            .border_color(gpui_kit::transparent_black())
            .child(
                h_flex()
                    .flex_1()
                    .justify_center()
                    // Balances the window controls on the right, so the name
                    // sits in the middle of the window.
                    .pl(px(WINDOW_CONTROLS_WIDTH))
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(APP_NAME),
            )
            .into_any_element()
    }

    /// Launchers (most games first) | chat apps on the left; theme, scan again and add game on the right.
    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let launcher_icons: Vec<AnyElement> = self
            .launchers
            .iter()
            .enumerate()
            .map(|(ix, launcher)| self.launcher_icon(("launcher", ix), launcher, cx))
            .collect();
        let social_icons: Vec<AnyElement> =
            self.social.iter().enumerate().map(|(ix, app)| self.launcher_icon(("social", ix), app, cx)).collect();
        let (theme_icon, theme_tip) = match self.settings.theme {
            ThemeChoice::System => (Icon::empty().path("mulch/monitor.svg"), "Theme: same as Windows"),
            ThemeChoice::Light => (Icon::new(IconName::Sun), "Theme: light"),
            ThemeChoice::Dark => (Icon::new(IconName::Moon), "Theme: dark"),
        };
        // Its tooltip reads the current text each frame, so it changes on click
        // while still showing.
        self.theme_tip.set(theme_tip);
        let tip = self.theme_tip.clone();
        let theme = div()
            .id("theme-tip")
            .child(
                Button::new("theme")
                    .ghost()
                    .icon(theme_icon)
                    .on_click(cx.listener(|app, _, window, cx| app.cycle_theme(window, cx))),
            )
            .tooltip(move |window, cx| {
                let tip = tip.clone();
                Tooltip::element(move |_, _| tip.get()).build(window, cx)
            })
            .tooltip_show_delay(TOOLTIP_DELAY)
            .into_any_element();
        let actions = vec![
            theme,
            quick_tooltip(
                "rescan-tip",
                "Scan again",
                Button::new("rescan")
                    .ghost()
                    .icon(IconName::RefreshCw)
                    .loading(self.scanning)
                    .on_click(cx.listener(|app, _, _, cx| app.rescan(cx))),
            )
            .into_any_element(),
            quick_tooltip(
                "add-game-tip",
                "Add game manually",
                Button::new("add-game")
                    .ghost()
                    .icon(IconName::Plus)
                    .on_click(cx.listener(|app, _, _, cx| app.open_add_panel(cx))),
            )
            .into_any_element(),
        ];

        let theme = cx.theme();
        // A floating glass pill over the grid, which scrolls underneath it.
        // Groups are told apart by a wider gap rather than dividers.
        let pill = h_flex()
            .absolute()
            .top(px(TITLE_BAR_HEIGHT + PILL_GAP))
            .left(px(GRID_MARGIN_X))
            .right(px(GRID_MARGIN_X))
            .h(px(TOOLBAR_HEIGHT))
            .px(px(TOOLBAR_PADDING))
            .gap(px(TOOLBAR_GAP))
            .rounded_full()
            // Solid enough that its icons stay clear over posters scrolling underneath.
            .bg(with_alpha(theme.popover, PILL_OPACITY))
            .border_1()
            .border_color(theme.border.opacity(0.6))
            .shadow_lg()
            .child(
                h_flex().flex_1().child(
                    self.measured(
                        0,
                        h_flex()
                            .gap(px(TOOLBAR_GAP))
                            .children(launcher_icons)
                            .child(h_flex().ml(px(TOOLBAR_GAP)).gap(px(TOOLBAR_GAP)).children(social_icons)),
                    ),
                ),
            )
            .child(self.measured(1, h_flex().gap_2().children(actions)));
        deferred(pill).into_any_element()
    }
    /// A toolbar group at its natural width, recording that width (in slot
    /// `ix`) each time it's laid out.
    fn measured(&self, ix: usize, group: Div) -> Div {
        let widths = self.toolbar_widths.clone();
        group.flex_shrink_0().relative().child(
            canvas(
                move |bounds, window, _| {
                    let width = f32::from(bounds.size.width);
                    if widths[ix].replace(width) != width {
                        window.refresh();
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .size_full(),
        )
    }

    /// Applies the chosen theme (for "system", whichever Windows is using).
    fn apply_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        Theme::change(theme_mode(self.settings.theme, window.appearance()), Some(window), cx);
        make_glassy(cx);
    }

    /// The theme button: system, then light, then dark, then system again.
    fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.theme = self.settings.theme.next();
        self.settings.save();
        self.fade_theme(window, cx);
    }

    /// Switches to the chosen theme, blending the colours over a moment.
    fn fade_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let from = self
            .theme_fade
            .as_ref()
            .map_or_else(|| cx.theme().colors, |(from, to, fade)| blend_colors(from, to, fade.now()));
        self.apply_theme(window, cx);
        let to = cx.theme().colors;
        let fade = Tween { from: 0., to: 1., since: std::time::Instant::now(), duration: THEME_FADE };
        self.theme_fade = Some((from, to, fade));
        Theme::global_mut(cx).colors = from;
        cx.notify();
    }

    /// Steps a theme fade on (each frame while one is running).
    fn step_theme_fade(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((from, to, fade)) = self.theme_fade else { return };
        let colors = if fade.done() { to } else { blend_colors(&from, &to, fade.now()) };
        let theme = Theme::global_mut(cx);
        theme.colors = colors;
        theme.tokens.background = colors.background.into();
        if fade.done() {
            self.theme_fade = None;
        } else {
            window.request_animation_frame();
        }
    }

    /// The narrowest the window can be with the whole toolbar still showing.
    fn toolbar_min_width(&self) -> f32 {
        GRID_MARGIN_X * 2.
            + TOOLBAR_PADDING * 2.
            + TOOLBAR_GAP
            + self.toolbar_widths[0].get()
            + self.toolbar_widths[1].get()
    }

    /// How many games were played in the last week, the last month (but not
    /// the last week) and before that or never. Games are sorted most recent first, so each
    /// group follows the one before.
    fn recency_groups(&self) -> [usize; 3] {
        const DAY: u64 = 24 * 60 * 60;
        let now = history::now();
        let mut groups = [0; 3];
        for game in &self.games {
            let age = self.history.last_played(game).map(|when| now.saturating_sub(when));
            let group = match age {
                Some(age) if age <= 7 * DAY => 0,
                Some(age) if age <= 30 * DAY => 1,
                _ => 2,
            };
            groups[group] += 1;
        }
        groups
    }

    /// Keeps the window at least as wide as the toolbar needs.
    fn update_min_width(&mut self, window: &Window) {
        if self.hwnd.is_none() {
            if let Ok(RawWindowHandle::Win32(handle)) = HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
                let hwnd = handle.hwnd.get();
                crate::window_size::install(hwnd);
                self.hwnd = Some(hwnd);
            }
        }
        if let Some(hwnd) = self.hwnd {
            crate::window_size::set_snap(GRID_MARGIN_X * 2., TILE_WIDTH + GRID_GAP, GRID_GAP);
            crate::window_size::set_min(hwnd, self.toolbar_min_width().max(MIN_WINDOW_WIDTH));
        }
    }

    /// "Add game manually": games found on this PC, ticked, plus a file picker
    /// for anything it missed.
    fn add_panel(&self, panel: &AddPanel, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let body = match &panel.suggestions {
            None => div()
                .text_sm()
                .text_color(muted)
                .child("Looking for games that aren't in Steam, Epic or other launchers…")
                .into_any_element(),
            Some(found) if found.is_empty() => div()
                .text_sm()
                .text_color(muted)
                .child("No games found outside your launchers. Use Browse to pick a game's .exe yourself.")
                .into_any_element(),
            Some(found) => v_flex()
                .id("suggestions")
                // Room for the rows' hover background, which reaches past their content.
                .mx(px(-8.))
                .px(px(8.))
                .max_h(px(320.))
                .overflow_y_scroll()
                .children(found.iter().enumerate().map(|(ix, suggestion)| {
                    let exe = suggestion.exe.clone();
                    let detail =
                        format!("{}\nLooks like a game: {}", suggestion.exe.display(), suggestion.reason).into();
                    check_row(
                        ("suggestion", ix),
                        panel.selected.contains(&suggestion.exe),
                        suggestion.name.clone(),
                        Some(detail),
                        cx,
                    )
                    .on_click(cx.listener(move |app, _, _, cx| app.toggle_suggestion(exe.clone(), cx)))
                }))
                .into_any_element(),
        };
        let can_add = panel.suggestions.is_some() && !panel.selected.is_empty();

        deferred(
            div()
                .id("add-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(with_alpha(theme.background, 0.85))
                .child(
                    v_flex()
                        .w(px(520.))
                        .p_6()
                        .gap_4()
                        .rounded_lg()
                        .bg(theme.popover)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_lg()
                        .child(div().text_xl().font_semibold().child("Add game manually"))
                        .child(body)
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new("add-browse")
                                        .outline()
                                        .icon(IconName::FolderOpen)
                                        .label("Browse…")
                                        .on_click(cx.listener(|app, _, _, cx| app.browse_for_games(cx))),
                                )
                                .child(div().flex_1())
                                .child(
                                    Button::new("add-cancel")
                                        .ghost()
                                        .label("Cancel")
                                        .on_click(cx.listener(|app, _, _, cx| app.close_add_panel(cx))),
                                )
                                .child(
                                    Button::new("add-selected")
                                        .primary()
                                        .label("Add selected")
                                        .disabled(!can_add)
                                        .on_click(cx.listener(|app, _, _, cx| app.add_selected(cx))),
                                ),
                        ),
                ),
        )
        .with_priority(4)
        .into_any_element()
    }
    fn launcher_icon(&self, id: (&'static str, usize), launcher: &Launcher, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = launcher.name;
        let open = launcher.open.clone();

        div()
            .id(id)
            .size(px(LAUNCHER_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .hover(|style| style.bg(theme.list_hover))
            .child(launcher_glyph(name, launcher.icon.clone(), theme.muted_foreground))
            .tooltip(move |window, cx| Tooltip::new(format!("Open {name}")).build(window, cx))
            .tooltip_show_delay(TOOLTIP_DELAY)
            .on_click(move |_, window, cx| run_action(&open, name, window, cx))
            .into_any_element()
    }

    fn tile(&self, ix: usize, game: &Game, layout: &GridLayout, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (width, height) = (layout.tile_width, layout.tile_width * COVER_ASPECT);

        let clicked = game.clone();
        let lift = Tween::value(&self.lifts, &game.id);
        // Glass buttons fade in over the poster while it's hovered.
        let (dx, dy) = if self.hovered_tile == Some(ix) { self.tilt } else { (0., 0.) };
        let cursor = ((dx + 1.) / 2. * width, (dy + 1.) / 2. * height);
        let actions = (lift > 0.).then(|| self.poster_actions(game, (width, height), cursor, cx).opacity(lift));
        let theme = cx.theme();
        let poster = div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .rounded(px(TILE_RADIUS))
            .bg(theme.muted)
            .child(artwork(game, width));

        // The full name shows on hover only when it's cut short.
        let hovered = self.hovered_tile == Some(ix);
        let (dx, dy) = if hovered { self.tilt } else { (0., 0.) };
        let show_full_name = hovered && name_is_cut_short(&game.name, width, window, cx);
        // The glow takes the poster's own colour where it has one.
        let accent = match &game.art {
            Some(Art::Cover(path) | Art::Icon(path)) => self.accents.get(path).copied(),
            None => None,
        };
        let glow = match accent {
            Some(color) => with_alpha(color, if theme.mode.is_dark() { 0.55 } else { 0.5 }),
            None if theme.mode.is_dark() => gpui_kit::white().opacity(0.25),
            None => gpui_kit::black().opacity(0.35),
        };
        let leaving = Tween::value(&self.leaving, &game.id);
        let staggered = self.first_shown.is_none_or(|at| at.elapsed() < STAGGER_WINDOW);
        div()
            .id(("game", ix))
            .relative()
            .w(px(width))
            .on_hover({
                let id = game.id.clone();
                cx.listener(move |app, hovered: &bool, _, cx| {
                    if *hovered {
                        app.hovered_tile = Some(ix);
                    } else if app.hovered_tile == Some(ix) {
                        app.hovered_tile = None;
                    }
                    // Moving away cancels a half-done remove.
                    if !*hovered && app.confirm_remove.as_deref() == Some(id.as_str()) {
                        app.confirm_remove = None;
                    }
                    Tween::go(&mut app.lifts, &id, if *hovered { 1. } else { 0. }, LIFT_DURATION);
                    app.tilt = (0., 0.);
                    cx.notify();
                })
            })
            .on_mouse_move(cx.listener(move |app, event: &MouseMoveEvent, _, cx| {
                let bounds = app.hovered_bounds.get();
                if app.hovered_tile != Some(ix) || bounds.size.width <= px(0.) {
                    return;
                }
                let across = (event.position.x - bounds.origin.x) / bounds.size.width;
                let down = (event.position.y - bounds.origin.y) / bounds.size.height;
                app.tilt = ((across * 2. - 1.).clamp(-1., 1.), (down * 2. - 1.).clamp(-1., 1.));
                cx.notify();
            }))
            // A double-click anywhere on the tile plays, as well as the Play button.
            .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
                if std::mem::take(&mut app.action_clicked) {
                    return;
                }
                if event.click_count() >= 2 {
                    app.play(&clicked, window, cx);
                }
            }))
            .child(
                // The poster lifts a little, with a soft glow, while hovered.
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .overflow_hidden()
                    .rounded(px(TILE_RADIUS))
                    .bg(theme.muted)
                    .child(poster)
                    .when(hovered, |frame| {
                        // Measure the poster, so the mouse's place on it is known.
                        let bounds = self.hovered_bounds.clone();
                        frame.child(canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {}).absolute().size_full())
                    })
                    // A soft sheen on the side the mouse is on.
                    .when(hovered, |frame| frame.child(sheen(dx, dy)))
                    .children(actions)
                    // Lifts, leans toward the mouse, and casts its shadow away from it.
                    .left(px(TILT_SHIFT * dx * lift))
                    .top(px((-LIFT + TILT_SHIFT * dy) * lift))
                    .shadow(vec![BoxShadow {
                        color: glow.opacity(lift),
                        offset: point(px(-TILT_SHADOW * dx * lift), px((8. - TILT_SHADOW * dy) * lift)),
                        blur_radius: px(24. * lift),
                        spread_radius: px(0.),
                        inset: false,
                    }]),
            )
            .child(
                // One line, cut short with "…"; hovering one that's cut short
                // shows the whole name in a pill whose text sits exactly over
                // it, running over the neighbouring tiles (drawn last so
                // nothing covers it).
                div()
                    .relative()
                    .h(px(NAME_LINE_HEIGHT))
                    .mt_2()
                    .text_sm()
                    .font_medium()
                    .child(div().truncate().child(game.name.clone()))
                    .when(show_full_name, |this| {
                        this.child(
                            deferred(
                                div()
                                    .absolute()
                                    // Offset by padding and border, so the text lines up exactly.
                                    .top(px(-NAME_PILL_PADDING.1 - 1.))
                                    .left(px(-NAME_PILL_PADDING.0 - 1.))
                                    .px(px(NAME_PILL_PADDING.0))
                                    .py(px(NAME_PILL_PADDING.1))
                                    .whitespace_nowrap()
                                    .rounded_full()
                                    // Solid, so the cut-short name underneath doesn't show through.
                                    .bg(with_alpha(theme.popover, 1.))
                                    .border_1()
                                    .border_color(theme.border)
                                    .shadow_md()
                                    .child(game.name.clone()),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(game.platform.label()))
            .h(px(height + LABEL_HEIGHT))
            // Tiles fade and rise into place when they first appear, one
            // shortly after another.
            // Removed: fades and sinks away.
            .when(leaving > 0., |tile| tile.opacity(1. - leaving).top(px(ENTER_RISE * leaving)))
            .with_animation(
                ElementId::Name(format!("enter-{}", game.id).into()),
                entrance(if staggered { ix } else { 0 }),
                |tile, t| tile.opacity(t).top(px(ENTER_RISE * (1. - t))),
            )
            .into_any_element()
    }
}

const LAUNCHER_SIZE: f32 = 36.;
/// A value easing from where it was toward a target (0 to 1).
#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    since: std::time::Instant,
    duration: std::time::Duration,
}

impl Tween {
    fn now(&self) -> f32 {
        let t = (self.since.elapsed().as_secs_f32() / self.duration.as_secs_f32()).min(1.);
        self.from + (self.to - self.from) * ease_out_quint()(t)
    }

    fn done(&self) -> bool {
        self.since.elapsed() >= self.duration
    }

    /// The value for `id` (0 if it has none).
    fn value(tweens: &HashMap<String, Tween>, id: &str) -> f32 {
        tweens.get(id).map_or(0., Tween::now)
    }

    /// Starts easing `id` toward `to` from wherever it is now.
    fn go(tweens: &mut HashMap<String, Tween>, id: &str, to: f32, duration: std::time::Duration) {
        let from = Tween::value(tweens, id);
        tweens.insert(id.to_string(), Tween { from, to, since: std::time::Instant::now(), duration });
    }

    /// Whether any are still moving (keeps finished ones).
    fn tidy_up(tweens: &mut HashMap<String, Tween>) -> bool {
        tweens.values().any(|t| !t.done())
    }

    /// Forgets finished tweens resting at 0; returns whether any are still moving.
    fn tidy(tweens: &mut HashMap<String, Tween>) -> bool {
        tweens.retain(|_, t| !(t.done() && t.to == 0.));
        tweens.values().any(|t| !t.done())
    }
}

/// How long a theme change blends, and a removed game fades.
const THEME_FADE: std::time::Duration = std::time::Duration::from_millis(320);
const LEAVE_DURATION: std::time::Duration = std::time::Duration::from_millis(260);
/// How long after games first appear that tiles still enter one by one.
const STAGGER_WINDOW: std::time::Duration = std::time::Duration::from_secs(2);

/// Theme colours part-way between two themes (`t` from 0 to 1).
fn blend_colors(from: &ThemeColor, to: &ThemeColor, t: f32) -> ThemeColor {
    let mix = |a: Hsla, b: Hsla| {
        let (a, b) = (Rgba::from(a), Rgba::from(b));
        let m = |x: f32, y: f32| x + (y - x) * t;
        Hsla::from(Rgba { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: m(a.a, b.a) })
    };
    let mut out = *to;
    macro_rules! blend { ($($field:ident),*) => { $(out.$field = mix(from.$field, to.$field);)* } }
    blend!(
        background,
        foreground,
        muted,
        muted_foreground,
        popover,
        popover_foreground,
        border,
        list_hover,
        title_bar,
        primary,
        primary_foreground,
        primary_hover,
        secondary,
        secondary_foreground,
        secondary_hover,
        accent,
        accent_foreground,
        danger,
        ring,
        input
    );
    out
}

/// Corner rounding for posters, panels and cards.
const TILE_RADIUS: f32 = 8.;

/// How far a hovered poster leans toward the mouse, and its shadow away.
const TILT_SHIFT: f32 = 3.;
const TILT_SHADOW: f32 = 6.;

/// A light sheen over a hovered poster, brightest on the side the mouse is
/// on (`dx`, `dy` from -1 to 1), stronger toward the edges.
fn sheen(dx: f32, dy: f32) -> Div {
    // CSS-style angle of the direction away from the mouse.
    let angle = (-dx).atan2(dy).to_degrees();
    let strength = 0.08 + 0.14 * (dx * dx + dy * dy).sqrt().min(1.);
    div().absolute().top_0().left_0().size_full().rounded(px(TILE_RADIUS)).bg(linear_gradient(
        angle,
        linear_color_stop(gpui_kit::white().opacity(strength), 0.),
        linear_color_stop(gpui_kit::white().opacity(0.), 0.6),
    ))
}

/// How far a hovered poster lifts, and how quickly.
const LIFT: f32 = 4.;
const LIFT_DURATION: std::time::Duration = std::time::Duration::from_millis(180);
/// Tiles' entrance: how far they rise, for how long, and the stagger between them.
const ENTER_RISE: f32 = 12.;
const ENTER_MS: f32 = 380.;
const ENTER_STAGGER_MS: f32 = 25.;
/// Tiles after this many all enter together, so a big library isn't slow to appear.
const ENTER_STAGGERED: usize = 24;

/// The entrance animation for the tile at `ix`: it waits its turn, then
/// eases in.
fn entrance(ix: usize) -> Animation {
    let delay = ix.min(ENTER_STAGGERED) as f32 * ENTER_STAGGER_MS;
    let total = delay + ENTER_MS;
    Animation::new(std::time::Duration::from_millis(total as u64))
        .with_easing(move |t| ease_out_quint()(((t * total - delay) / ENTER_MS).clamp(0., 1.)))
}

/// A game name's line under its tile (text_sm).
const NAME_LINE_HEIGHT: f32 = 20.;
/// Padding around a hovered game's full name (horizontal, vertical; the border adds 1).
const NAME_PILL_PADDING: (f32, f32) = (9., 3.);
/// Labels for the recency groups (see `recency_groups`).
const GROUP_NAMES: [&str; 3] = ["Played in the last week", "Played in the last month", "Everything else"];

/// How much of a tile's width an icon (rather than cover art) takes up.
const ICON_SHARE: f32 = 0.6;
/// Glass buttons over a hovered poster: sizes, spacing and distance from its bottom.
const GLASS_BUTTON: f32 = 36.;
const GLASS_PLAY_BUTTON: f32 = 48.;
const GLASS_GAP: f32 = 10.;
const GLASS_INSET: f32 = 14.;
/// Space above and below the grid.
const GRID_PADDING: f32 = 20.;
/// Least space either side of the grid (the window snaps to widths where it's exactly this).
const GRID_MARGIN_X: f32 = 24.;
/// The title bar's height.
const TITLE_BAR_HEIGHT: f32 = 34.;
/// The toolbar under the title bar.
const TOOLBAR_HEIGHT: f32 = 52.;
/// Space around the floating toolbar (chosen so it takes the same room as
/// the grid's top padding did).
const PILL_GAP: f32 = GRID_PADDING / 2.;
/// The floating toolbar's least opacity.
const PILL_OPACITY: f32 = 0.85;
const TOOLBAR_PADDING: f32 = 16.;
const TOOLBAR_GAP: f32 = 16.;
/// The window is never narrower than this, even with a short toolbar.
const MIN_WINDOW_WIDTH: f32 = 480.;
/// Minimise, maximise and close, on the right of the title bar, less its left padding.
const WINDOW_CONTROLS_WIDTH: f32 = 3. * 34. - 12.;

/// Makes the theme see-through, for the window's frosted-glass backdrop.
///
/// The glass sits over whatever is behind the window, so its tint is worked
/// out against the worst case (a white desktop behind dark glass, black
/// behind light): just strong enough that text keeps at least 7:1 contrast
/// and secondary text at least 4.5:1 (WCAG AAA and AA), whatever is behind.
/// Menus are worked out the same way.
fn make_glassy(cx: &mut App) {
    let theme = Theme::global_mut(cx);
    let worst = if theme.mode.is_dark() { gpui_kit::white() } else { gpui_kit::black() };
    let foreground = theme.colors.foreground;

    let tint = with_alpha(theme.colors.background, 1.);
    let alpha = glass_alpha(foreground, tint, worst, TEXT_CONTRAST);
    let background = tint.opacity(alpha);
    theme.colors.background = background;
    theme.tokens.background = background.into();
    theme.colors.title_bar = gpui_kit::transparent_black();
    theme.colors.muted_foreground = secondary_text(foreground, tint, over(tint, alpha, worst));

    let popover = with_alpha(theme.colors.popover, 1.);
    theme.colors.popover = popover.opacity(glass_alpha(foreground, popover, worst, TEXT_CONTRAST));
}

/// color at exactly lpha (gpui's opacity multiplies the existing alpha instead).
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// Body text contrast (WCAG AAA), and secondary text (AA).
const TEXT_CONTRAST: f32 = 7.;
const SECONDARY_CONTRAST: f32 = 4.5;

/// The least opacity for `tint` over `backdrop` that keeps `text` at `ratio`.
fn glass_alpha(text: Hsla, tint: Hsla, backdrop: Hsla, ratio: f32) -> f32 {
    (0..=100).map(|step| step as f32 / 100.).find(|&a| contrast(text, over(tint, a, backdrop)) >= ratio).unwrap_or(1.)
}

/// Secondary text: as far from `text` toward `tint` as still keeps AA contrast
/// against `background`.
fn secondary_text(text: Hsla, tint: Hsla, background: Hsla) -> Hsla {
    (0..=60)
        .rev()
        .map(|step| step as f32 / 100.)
        .map(|m| over(tint, m, text))
        .find(|&c| contrast(c, background) >= SECONDARY_CONTRAST)
        .unwrap_or(text)
}

/// `top` at `alpha` over `bottom`, as an opaque colour.
fn over(top: Hsla, alpha: f32, bottom: Hsla) -> Hsla {
    let (t, b) = (Rgba::from(top), Rgba::from(bottom));
    let mix = |x: f32, y: f32| x * alpha + y * (1. - alpha);
    Hsla::from(Rgba { r: mix(t.r, b.r), g: mix(t.g, b.g), b: mix(t.b, b.b), a: 1. })
}

/// WCAG contrast ratio between two opaque colours.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let luminance = |c: Hsla| {
        let c = Rgba::from(c);
        let linear = |v: f32| if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
        0.2126 * linear(c.r) + 0.7152 * linear(c.g) + 0.0722 * linear(c.b)
    };
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}
/// The light or dark theme for a choice, given whether Windows is in dark mode.
fn theme_mode(choice: ThemeChoice, windows: WindowAppearance) -> ThemeMode {
    match choice {
        ThemeChoice::Light => ThemeMode::Light,
        ThemeChoice::Dark => ThemeMode::Dark,
        ThemeChoice::System => match windows {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            _ => ThemeMode::Light,
        },
    }
}

/// Opening size, at the tile size: exactly 6 games across, and tall
/// enough for the three labelled groups (last week, last month, everything
/// else) with one row each, the last cut off halfway so it's clear there's
/// more below. Smaller if the screen is.
fn default_window_size(cx: &App) -> gpui_kit::Size<Pixels> {
    let tile = TILE_WIDTH;
    const COLUMNS: usize = 6;
    const GROUPS: f32 = 3.;
    let width = GRID_MARGIN_X * 2. + grid_width(COLUMNS, tile);
    let row = tile * COVER_ASPECT + LABEL_HEIGHT;
    let height = TITLE_BAR_HEIGHT
        + TOOLBAR_HEIGHT
        + GRID_PADDING
        + GROUPS * HEADING_HEIGHT
        + (GROUPS - 0.5) * row
        + (GROUPS - 1.) * GRID_GAP;
    let screen = cx.primary_display().map(|d| d.bounds().size);
    let (max_width, max_height) =
        screen.map(|s| (f32::from(s.width), f32::from(s.height) * 0.95)).unwrap_or((width, height));
    size(px(width.min(max_width)), px(height.min(max_height)))
}

/// Cover art fills as much of the tile as it can without cropping. Icons (already trimmed of transparent padding)
/// sit centred at a fixed share of the tile, so they all look the same size.
/// No words inside the tile: the name is always shown underneath.
fn artwork(game: &Game, width: f32) -> AnyElement {
    match &game.art {
        // Shown whole: scaled to fit inside the tile, never cropped or stretched.
        Some(Art::Cover(path)) => {
            img(path.clone()).size_full().rounded(px(TILE_RADIUS)).object_fit(ObjectFit::Contain).into_any_element()
        }
        Some(Art::Icon(path)) => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(img(path.clone()).size(px(width * ICON_SHARE)).object_fit(ObjectFit::Contain))
            .into_any_element(),
        None => div().size_full().into_any_element(),
    }
}

/// The icon (or initials) for a launcher button.
fn launcher_glyph(name: &str, icon: Option<PathBuf>, muted: Hsla) -> AnyElement {
    match icon {
        Some(path) => img(path).size(px(LAUNCHER_SIZE - 10.)).object_fit(ObjectFit::Contain).into_any_element(),
        None => div()
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .child(name.chars().filter(|c| c.is_uppercase()).take(2).collect::<String>())
            .into_any_element(),
    }
}

/// Tooltips show after this long (the UI kit's buttons wait half a second).
const TOOLTIP_DELAY: std::time::Duration = std::time::Duration::from_millis(150);

/// Wraps an element with a tooltip that shows after `TOOLTIP_DELAY`.
fn quick_tooltip(id: &'static str, text: &'static str, child: impl IntoElement) -> Stateful<Div> {
    div()
        .id(id)
        .child(child)
        .tooltip(move |window, cx| Tooltip::new(text).build(window, cx))
        .tooltip_show_delay(TOOLTIP_DELAY)
}

/// Whether a game's name (text_sm, medium weight) is too wide for its tile.
fn name_is_cut_short(name: &str, width: f32, window: &Window, cx: &App) -> bool {
    text_width(name, window, cx) > width
}

/// Width of a line of text_sm, medium-weight text.
fn text_width(text: &str, window: &Window, cx: &App) -> f32 {
    let font = Font { weight: FontWeight::MEDIUM, ..font(cx.theme().font_family.clone()) };
    let run = TextRun {
        len: text.len(),
        font,
        color: Hsla::default(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let font_size = rems(0.875).to_pixels(window.rem_size());
    f32::from(window.text_system().shape_line(SharedString::from(text.to_string()), font_size, &[run], None).width)
}

/// A round dark-glass button with a white icon, readable over any poster.
/// `danger` tints it red (a remove waiting to be confirmed).
fn glass_button(id: &'static str, icon: IconName, size: f32, danger: bool) -> Stateful<Div> {
    let fill = if danger { gpui_kit::red().opacity(0.75) } else { gpui_kit::black().opacity(0.45) };
    let hover = if danger { gpui_kit::red().opacity(0.9) } else { gpui_kit::black().opacity(0.65) };
    div()
        .id(id)
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fill)
        .border_1()
        .border_color(gpui_kit::white().opacity(0.35))
        .shadow_md()
        .text_color(gpui_kit::white())
        .hover(move |style| style.bg(hover).border_color(gpui_kit::white().opacity(0.6)))
        .child(Icon::new(icon).size(px(size * 0.42)))
}

/// Adds a quick tooltip (after `TOOLTIP_DELAY`).
fn with_tooltip(element: Stateful<Div>, text: impl Into<SharedString>) -> Stateful<Div> {
    let text: SharedString = text.into();
    element.tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx)).tooltip_show_delay(TOOLTIP_DELAY)
}

/// A full-width option: a checkbox beside a title (and optional detail lines
/// under it). The whole row is clickable and highlights on hover.
fn check_row(
    id: impl Into<ElementId>,
    checked: bool,
    title: impl Into<SharedString>,
    detail: Option<SharedString>,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let (hover, muted) = (theme.list_hover, theme.muted_foreground);
    h_flex()
        .id(id)
        .w_full()
        .gap_3()
        .px_2()
        // The hover background reaches past the content, which lines up with the text around it.
        .mx(px(-8.))
        .py_1p5()
        .rounded_md()
        .hover(move |style| style.bg(hover))
        .child(Checkbox::new("check").checked(checked).large())
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().font_medium().child(title.into()))
                .children(detail.map(|detail| div().text_xs().text_color(muted).child(detail))),
        )
}

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_min_width(window);
        self.step_theme_fade(window, cx);
        if Tween::tidy(&mut self.lifts) | Tween::tidy_up(&mut self.leaving) {
            window.request_animation_frame();
        }
        let viewport = window.viewport_size();
        let layout = grid_layout(&self.recency_groups(), f32::from(viewport.width) - GRID_MARGIN_X * 2.);
        let title_bar = self.title_bar(cx);
        let toolbar = self.toolbar(cx);
        // Rows built explicitly (rather than by wrapping) so each group starts a new row.
        let mut tiles: Vec<AnyElement> =
            self.games.iter().enumerate().map(|(ix, game)| self.tile(ix, game, &layout, window, cx)).collect();
        let muted = cx.theme().muted_foreground;
        let mut sections = Vec::new();
        for section in &layout.sections {
            let mut rows = Vec::new();
            for &size in &section.rows {
                let rest = tiles.split_off(size.min(tiles.len()));
                rows.push(h_flex().items_start().gap(px(GRID_GAP)).children(std::mem::replace(&mut tiles, rest)));
            }
            // Labels and rows start at the grid's left edge.
            sections.push(
                v_flex()
                    .gap(px(GRID_GAP))
                    .when(layout.labelled, |this| {
                        this.child(
                            div()
                                .h(px(HEADING_HEIGHT - GRID_GAP))
                                .text_sm()
                                .font_semibold()
                                .text_color(muted)
                                .child(GROUP_NAMES[section.group]),
                        )
                    })
                    .children(rows),
            );
        }
        let empty = !self.scanning && self.games.is_empty();

        let add_panel = self.add_panel.as_ref().map(|panel| self.add_panel(panel, cx));

        v_flex().relative().size_full().children(add_panel).child(title_bar).child(toolbar).child(
            div()
                .id("library")
                .flex_1()
                .overflow_y_scroll()
                // Room at the top for the floating toolbar.
                .pt(px(PILL_GAP * 2. + TOOLBAR_HEIGHT))
                .pb(px(GRID_PADDING))
                .px(px(GRID_MARGIN_X))
                // Played in the last week, then the last month, then the rest.
                // The grid is centred (exactly, given the window's snapped widths).
                .flex()
                .items_start()
                .justify_center()
                .when(empty, |this| {
                    this.child(
                        v_flex()
                            .size_full()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .child(div().text_lg().child("No games found yet"))
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Install a game with any launcher, or use Add game manually."),
                            ),
                    )
                })
                .child(
                    v_flex()
                        .flex_shrink_0()
                        .w(px(grid_width(layout.columns, layout.tile_width)))
                        .gap(px(GRID_GAP))
                        .children(sections),
                ),
        )
    }
}
