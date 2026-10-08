//! The main window: a slim title bar, a toolbar (chat apps and launchers on
//! the left, buttons on the right) and every detected game as a tile, grouped
//! by when it was last played and sized so they all fit if they can.

use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::spinner::Spinner;
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
use mulch_launcher::restore::{Restore, WindowState};
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

/// `restore`: set when restarting after an update, to reopen where the old
/// copy was, in the background.
pub fn run(restore: Option<Restore>) {
    mulch_launcher::trace::mark("creating app");
    let app = gpui_kit::application();
    mulch_launcher::trace::mark("app created");
    app.with_assets(crate::assets::Assets).run(move |cx| {
        mulch_launcher::trace::mark("app running");
        gpui_kit::init(cx);
        mulch_launcher::trace::mark("kit init");
        Theme::change(theme_mode(Settings::load().theme, cx.window_appearance()), None, cx);
        finish_theme(cx);

        let options = WindowOptions {
            // Our own slim title bar (see `title_bar`), with the toolbar under
            // it. The title is still set for the taskbar and Alt+Tab.
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            window_background: WindowBackgroundAppearance::Opaque,
            window_bounds: Some(match restore {
                Some(r) => {
                    let bounds = Bounds::new(point(px(r.x), px(r.y)), size(px(r.width), px(r.height)));
                    if r.state == WindowState::Maximized {
                        WindowBounds::Maximized(bounds)
                    } else {
                        WindowBounds::Windowed(bounds)
                    }
                }
                None => WindowBounds::Windowed(Bounds::centered(None, default_window_size(cx), cx)),
            }),
            // Restarted for an update: created hidden, then shown behind the
            // window the user is in (see update_min_width).
            show: restore.is_none(),
            focus: restore.is_none(),
            // The width also grows to fit the toolbar (see `update_min_width`).
            window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(360.))),
            app_id: Some(APP_NAME.into()),
            ..Default::default()
        };
        mulch_launcher::trace::mark("opening window");
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| MulchApp::new(restore, window, cx)))
            .expect("failed to open the window");
        mulch_launcher::trace::mark("window opened");
    });
}

/// The one window shown when the downloaded copy is run: an "Add to desktop"
/// checkbox (on by default) and Install. Closing it installs nothing.
pub fn run_installer() {
    gpui_kit::application().with_assets(crate::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(theme_mode(Settings::load().theme, cx.window_appearance()), None, cx);
        finish_theme(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            window_background: WindowBackgroundAppearance::Opaque,
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
                    .child(div().text_2xl().font_bold().child(format!("Install {APP_NAME}")))
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
                            pill_button("install", "Install", PillKind::Primary, false, cx)
                                .px(px(28.))
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
    /// The native window, once known.
    hwnd: Option<isize>,
    /// Launcher icons resampled to exactly their size on screen (by original),
    /// and the display scale they were made for.
    sized_icons: HashMap<PathBuf, PathBuf>,
    icon_scale: f32,
    /// The game tile under the mouse, which shows its full name.
    hovered_tile: Option<usize>,
    scanning: bool,
    rescan_requested: bool,
    artwork_initialized: bool,
    /// The added game whose Remove button was clicked once (the next click removes).
    confirm_remove: Option<String>,
    /// Reopening after an update: show the window in the background once it exists.
    restore: Option<Restore>,
    /// An update is installed: restart into it when the user isn't using the app.
    update_ready: bool,
    /// Whether the window is the active one (it only re-checks the library then).
    active: bool,
    /// A quiet re-check of the library is running.
    checking: bool,
    /// Games still waiting for their poster (being looked up or downloaded):
    /// they show a spinner rather than a stand-in icon until it arrives.
    art_pending: HashSet<String>,
    /// Each launcher's icon as a one-colour glyph (light ink, dark ink), for poster buttons.
    glyphs: HashMap<Platform, (PathBuf, PathBuf)>,
    /// When a poster button was last pressed, so the click on the tile under
    /// it doesn't also count (as half of a double-click to play).
    action_pressed: Option<std::time::Instant>,
    /// When each game was last played, for sorting most recent first.
    history: History,
    /// The theme the user chose (light, dark or following Windows), remembered.
    settings: Settings,
    /// The theme button's tooltip text, read as it's drawn so it updates while showing.
    theme_tip: Rc<Cell<&'static str>>,
    _subscriptions: Vec<Subscription>,
}

/// How often an installed copy checks for an update.
const UPDATE_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

/// How often to re-check the installed games while the window is active.
const LIBRARY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// What identifies the library: each game's id and name, in order.
fn library_signature(games: &[Game]) -> Vec<(String, String)> {
    let mut ids: Vec<(String, String)> = games.iter().map(|g| (g.id.clone(), g.name.clone())).collect();
    ids.sort();
    ids
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
    fn new(restore: Option<Restore>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        mulch_launcher::trace::mark("app new");
        let mut app = Self {
            games: Vec::new(),
            launchers: Vec::new(),
            social: Vec::new(),
            add_panel: None,
            settings: Settings::load(),
            theme_tip: Rc::default(),
            hwnd: None,
            sized_icons: HashMap::new(),
            icon_scale: 1.,
            hovered_tile: None,
            scanning: false,
            rescan_requested: false,
            artwork_initialized: false,
            confirm_remove: None,
            restore,
            update_ready: false,
            active: true,
            checking: false,
            art_pending: HashSet::new(),
            glyphs: HashMap::new(),
            action_pressed: None,
            history: History::load(),
            // Coming back to the window (e.g. after playing): re-sort so the
            // game just played is first.
            _subscriptions: vec![
                // Follow Windows switching between light and dark.
                cx.observe_window_appearance(window, |app, window, cx| {
                    app.apply_theme(window, cx);
                    cx.notify();
                }),
                cx.observe_window_activation(window, |app, window, cx| {
                    app.active = window.is_window_active();
                    // An update waiting: restart as soon as the user moves on.
                    if !app.active && app.update_ready {
                        app.restart_for_update(window, cx);
                        return;
                    }
                    if app.active {
                        app.resort(cx);
                        app.check_for_changes(cx);
                    }
                }),
            ],
        };
        app.apply_theme(window, cx);
        // Last time's library, posters and all, on the very first frame; the
        // scan that follows brings it up to date.
        if let Some(games) = load_library() {
            app.games = games;
            app.history.sort(&mut app.games);
        }
        app.rescan(cx);
        app.watch_for_running_games(cx);
        app.watch_for_library_changes(cx);
        app.watch_for_updates(window, cx);
        mulch_launcher::trace::mark("app new done");
        app
    }

    /// Every so often, records any game with a process running from its
    /// folder as played. This catches games started from their own launcher
    /// too, on every platform.
    /// Installed copies check for a newer release at startup and every few
    /// hours, download and install it in the background, then restart into
    /// it: at once if the user isn't using the app, otherwise as soon as they
    /// switch away. The restart reopens in the background, where it was.
    fn watch_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        // The Microsoft Store updates its own copy.
        if mulch_core::paths::is_packaged() {
            return;
        }
        let Some(dir) = install::install_dir().filter(|_| install::is_installed_copy()) else { return };
        mulch_update::clean_up(&dir);
        cx.spawn_in(window, async move |this, cx| {
            loop {
                let dir_ = dir.clone();
                let installed = cx
                    .background_spawn(async move {
                        let release = mulch_update::newer_release(env!("CARGO_PKG_VERSION"))?;
                        mulch_update::install(&release, &dir_).ok()
                    })
                    .await;
                if installed.is_some() {
                    this.update_in(cx, |app, window, cx| {
                        app.update_ready = true;
                        if !app.active {
                            app.restart_for_update(window, cx);
                        }
                    })
                    .ok();
                    break;
                }
                cx.background_executor().timer(UPDATE_CHECK_INTERVAL).await;
            }
        })
        .detach();
    }

    /// Starts the newly installed copy where this window is, then quits.
    fn restart_for_update(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(exe) = install::install_dir().map(|dir| mulch_update::exe_in(&dir)) else { return };
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Maximized(b) | WindowBounds::Fullscreen(b) => (b, true),
            WindowBounds::Windowed(b) => (b, false),
        };
        let minimized = self.hwnd.is_some_and(crate::window_size::is_minimized);
        let restore = Restore {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            state: if minimized {
                WindowState::Minimized
            } else if maximized {
                WindowState::Maximized
            } else {
                WindowState::Normal
            },
        };
        let started = std::process::Command::new(&exe).arg("--restore").arg(restore.to_arg()).spawn();
        if started.is_ok() {
            cx.quit();
        }
    }

    /// While the window is active, quietly re-checks which games are
    /// installed every few seconds, so installs and uninstalls show up on
    /// their own. A check is a ~0.1 s scan on a background thread.
    fn watch_for_library_changes(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(LIBRARY_CHECK_INTERVAL).await;
                if this.update(cx, |app, cx| app.check_for_changes(cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Scans in the background and refreshes only if the installed games
    /// changed (so nothing on screen moves otherwise).
    fn check_for_changes(&mut self, cx: &mut Context<Self>) {
        if !self.active || self.scanning || self.checking {
            return;
        }
        self.checking = true;
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move {
                    let started = std::time::Instant::now();
                    let found = library_signature(&scan::scan_all(&[]).games);
                    mulch_launcher::trace::mark(&format!("library check {:?}", started.elapsed()));
                    found
                })
                .await;
            this.update(cx, |app, cx| {
                app.checking = false;
                if found != library_signature(&app.games) {
                    app.rescan(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn watch_for_running_games(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(PLAY_CHECK_INTERVAL).await;
                let Ok(games) = this.update(cx, |app, _| app.games.clone()) else { break };
                let running = cx
                    .background_spawn(async move {
                        let started = std::time::Instant::now();
                        let running = history::running_games(&games);
                        mulch_launcher::trace::mark(&format!("running games check {:?}", started.elapsed()));
                        running
                    })
                    .await;
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
            self.rescan_requested = true;
            return;
        }
        self.scanning = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { scan::scan_all(&[]) }).await;
            let Ok((mut games, mut launchers)) = this.update(cx, |app, cx| app.apply(result, cx)) else {
                return;
            };

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
                    if !games.is_empty() {
                        posters::fill_missing(&mut games);
                    }
                    games
                })
                .await;
            this.update(cx, |app, cx| {
                app.art_pending.clear();
                app.apply_art(games, launchers, cx);
                app.scanning = false;
                if std::mem::take(&mut app.rescan_requested) {
                    app.rescan(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn apply(&mut self, mut result: ScanResult, cx: &mut Context<Self>) -> (Vec<Game>, Vec<Launcher>) {
        mulch_launcher::trace::mark("scan applied");
        // Keep the art already showing (icons and downloaded posters arrive
        // after the scan), so a rescan doesn't blank every tile for a moment.
        let wanted = scan::prepare_artwork_refresh(&mut result.games, &self.games, !self.artwork_initialized);
        self.artwork_initialized = true;
        let shown_icons: HashMap<&str, PathBuf> = self
            .launchers
            .iter()
            .chain(&self.social)
            .filter_map(|launcher| Some((launcher.name, launcher.icon.clone()?)))
            .collect();
        let mut wanted_launchers = Vec::new();
        for launcher in result.launchers.iter_mut().chain(&mut result.social) {
            if let Some(icon) = shown_icons.get(launcher.name) {
                launcher.icon = Some(icon.clone());
            } else {
                wanted_launchers.push(launcher.clone());
            }
        }
        self.games = result.games;
        // So Show in Xbox app opens at once when clicked.
        launch::look_up_xbox_pages(
            self.games
                .iter()
                .filter_map(|g| match &g.show_in_launcher {
                    Some(Action::XboxAppPage(name)) => Some(name.clone()),
                    _ => None,
                })
                .collect(),
        );
        // Only games with nothing to show yet (new ones) wait for their art;
        // everything already showing stays exactly as it is.
        self.art_pending = wanted.iter().filter(|g| g.art.is_none()).map(|g| g.id.clone()).collect();
        self.history.sort(&mut self.games);
        self.launchers = result.launchers;
        // Launchers with the most games first; ties alphabetical.
        let games_on =
            |launcher: &Launcher| self.games.iter().filter(|g| Some(g.platform) == launcher.platform).count();
        let mut launchers = std::mem::take(&mut self.launchers);
        launchers.sort_by_cached_key(|l| (std::cmp::Reverse(games_on(l)), l.name.to_lowercase()));
        self.launchers = launchers;
        self.social = result.social;
        self.confirm_remove = None;
        cx.notify();
        (wanted, wanted_launchers)
    }

    fn play(&mut self, game: &Game, window: &mut Window, cx: &mut Context<Self>) {
        run_action(&game.launch, &game.name, window, cx);
        self.history.record(&game.id, history::now());
        self.history.save();
        cx.notify();
    }

    /// A glass button whose height springs between resting, hovered and
    /// pressed as the mouse moves over it and presses it.
    #[allow(clippy::too_many_arguments)]
    fn glass(
        &self,
        game_id: &str,
        which: impl Into<ElementId>,
        content: AnyElement,
        size: f32,
        danger: bool,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let _ = (game_id, cx);
        glass_button(which, content, size, danger, dark)
    }

    /// Buttons over a hovered poster: Play, big, centred; below it Show in its
    /// launcher (one per launcher that has it), or for games the user added,
    /// Remove (which asks again first).
    fn poster_actions(&self, game: &Game, size: (f32, f32), cx: &mut Context<Self>) -> Div {
        let dark = cx.theme().mode.is_dark();
        let mut actions: Vec<AnyElement> = Vec::new();
        // One button per launcher that has the game (usually one; a game two
        // launchers installed gets a smaller button for each, side by side).
        let copies: Vec<(Platform, Action)> = std::iter::once((game.platform, game.show_in_launcher.clone()))
            .chain(game.other_copies.iter().cloned())
            .filter_map(|(platform, show)| Some((platform, show?)))
            .collect();
        let button_size = if copies.len() > 1 { GLASS_SMALL_BUTTON } else { GLASS_BUTTON };
        for (ix, (platform, show)) in copies.into_iter().enumerate() {
            let launcher = match platform {
                Platform::Xbox => "the Xbox app",
                Platform::Gog => "GOG Galaxy",
                other => other.label(),
            };
            let name = game.name.clone();
            // The launcher's own logo, as a glyph in the button's ink.
            let glyph =
                self.glyphs.get(&platform).map(|(on_dark, on_light)| if dark { on_dark } else { on_light }.clone());
            let content = match glyph {
                Some(glyph) => {
                    img(glyph).size(px(button_size * GLYPH_FILL)).object_fit(ObjectFit::Contain).into_any_element()
                }
                None => Icon::new(IconName::ExternalLink).size(px(button_size * ICON_FILL)).into_any_element(),
            };
            let button =
                self.glass(&game.id, ("show-launcher", ix), content, button_size, false, dark, cx).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, window, cx| {
                        app.action_pressed = Some(std::time::Instant::now());
                        run_action(&show, &name, window, cx);
                    }),
                );
            actions
                .push(glass_slot(button_size, with_tooltip(button, format!("Show in {launcher}"))).into_any_element());
        }
        if actions.is_empty()
            && let (Platform::Manual, Action::Exe { path, .. }) = (game.platform, &game.launch)
        {
            let exe = path.clone();
            let id = game.id.clone();
            let confirming = self.confirm_remove.as_deref() == Some(game.id.as_str());
            let button = self
                .glass(
                    &game.id,
                    "remove",
                    icon_content(IconName::Close, GLASS_BUTTON),
                    GLASS_BUTTON,
                    confirming,
                    dark,
                    cx,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, _, cx| {
                        app.action_pressed = Some(std::time::Instant::now());
                        if app.confirm_remove.as_deref() == Some(id.as_str()) {
                            app.confirm_remove = None;
                            app.remove_manual(exe.clone(), cx);
                        } else {
                            app.confirm_remove = Some(id.clone());
                            cx.notify();
                        }
                    }),
                );
            let tip = if confirming { "Click again to remove" } else { "Remove" };
            actions.push(glass_slot(GLASS_BUTTON, with_tooltip(button, tip)).into_any_element());
        }

        // Play, big, in the middle of the space above the bottom row.
        let (width, height) = size;
        let half = GLASS_PLAY_BUTTON / 2.;
        let row = GLASS_INSET + GLASS_BUTTON;
        let (x, y) = (width / 2., (height - row) / 2.);
        let g = game.clone();
        let play = self
            .glass(
                &game.id,
                "play",
                Icon::empty().path("mulch/play-filled.svg").size(px(GLASS_PLAY_BUTTON * ICON_FILL)).into_any_element(),
                GLASS_PLAY_BUTTON,
                false,
                dark,
                cx,
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, window, cx| {
                    app.action_pressed = Some(std::time::Instant::now());
                    app.play(&g, window, cx);
                }),
            );
        let play =
            glass_slot(GLASS_PLAY_BUTTON, with_tooltip(play, "Play")).absolute().left(px(x - half)).top(px(y - half));

        div().absolute().top_0().left_0().size_full().child(play).child(
            h_flex().absolute().left_0().right_0().bottom(px(GLASS_INSET)).justify_center().gap_2().children(actions),
        )
    }
    fn apply_art(&mut self, games: Vec<Game>, launchers: Vec<Launcher>, cx: &mut Context<Self>) {
        // Icons may have been added or replaced with trimmed copies.
        let game_art: HashMap<String, Art> = games.into_iter().filter_map(|g| Some((g.id, g.art?))).collect();
        for game in &mut self.games {
            if let Some(art) = game_art.get(&game.id) {
                if !matches!(game.art, Some(Art::Cover(_))) || matches!(art, Art::Cover(_)) {
                    game.art = Some(art.clone());
                }
            }
        }
        let launcher_icons: HashMap<&str, PathBuf> =
            launchers.into_iter().filter_map(|l| Some((l.name, l.icon?))).collect();
        for launcher in self.launchers.iter_mut().chain(&mut self.social) {
            if let Some(icon) = launcher_icons.get(launcher.name) {
                launcher.icon = Some(icon.clone());
            }
        }
        self.make_glyphs(cx);
        self.size_icons(cx);
        save_library(self.games.clone());
        cx.notify();
    }

    /// Resamples launcher icons to exactly their size on screen, in the
    /// background (drawn scaled, they look soft and jagged).
    fn size_icons(&mut self, cx: &mut Context<Self>) {
        let size = (LAUNCHER_ICON * self.icon_scale).round() as u32;
        let wanted: Vec<PathBuf> = self
            .launchers
            .iter()
            .chain(&self.social)
            .filter_map(|l| l.icon.clone())
            .filter(|icon| !self.sized_icons.contains_key(icon))
            .collect();
        if wanted.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move {
                    wanted
                        .into_iter()
                        .filter_map(|icon| Some((icon.clone(), art::sized(&icon, size)?)))
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |app, cx| {
                app.sized_icons.extend(made);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Makes one-colour glyphs of the launchers' icons, in the background.
    fn make_glyphs(&mut self, cx: &mut Context<Self>) {
        let wanted: Vec<(Platform, PathBuf)> = self
            .launchers
            .iter()
            .filter_map(|l| Some((l.platform?, l.icon.clone()?)))
            .filter(|(platform, _)| !self.glyphs.contains_key(platform))
            .collect();
        if wanted.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let made = cx
                .background_spawn(async move {
                    wanted
                        .into_iter()
                        .filter_map(|(platform, icon)| {
                            Some((platform, (art::glyph(&icon, GLYPH_ON_DARK)?, art::glyph(&icon, GLYPH_ON_LIGHT)?)))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            this.update(cx, |app, cx| {
                app.glyphs.extend(made);
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

    /// Removes a game added by hand.
    fn remove_manual(&mut self, exe: PathBuf, cx: &mut Context<Self>) {
        let id = self
            .games
            .iter()
            .find(|g| matches!(&g.launch, Action::Exe { path, .. } if *path == exe))
            .map(|g| g.id.clone());
        if let Some(id) = id {
            self.games.retain(|g| g.id != id);
        }
        let _ = manual::remove(&exe);
        self.rescan(cx);
        cx.notify();
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        // The window's own background, with a line under it.
        TitleBar::new()
            .bg(gpui_kit::transparent_black())
            .border_color(theme.border)
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

    /// The bar under the title bar, always showing: the theme on the left,
    /// the launchers and chat apps in the middle, Add a game on the right.
    /// (The library re-checks itself.)
    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let add = quick_tooltip(
            "add-game-tip",
            "Add game manually",
            bar_button("add-game", Icon::new(IconName::Plus), cx)
                .on_mouse_down(MouseButton::Left, cx.listener(|app, _, _, cx| app.open_add_panel(cx))),
        );
        let (theme_icon, theme_tip) = match self.settings.theme {
            ThemeChoice::System => (Icon::empty().path("mulch/monitor.svg"), "Theme: same as Windows"),
            ThemeChoice::Light => (Icon::new(IconName::Sun), "Theme: light"),
            ThemeChoice::Dark => (Icon::new(IconName::Moon), "Theme: dark"),
        };
        // Its tooltip reads the current text as it's drawn, so it changes on
        // click while still showing.
        self.theme_tip.set(theme_tip);
        let tip = self.theme_tip.clone();
        let theme = bar_button("theme", theme_icon, cx)
            .on_mouse_down(MouseButton::Left, cx.listener(|app, _, window, cx| app.cycle_theme(window, cx)))
            .tooltip(move |window, cx| {
                let tip = tip.clone();
                Tooltip::element(move |_, _| tip.get()).build(window, cx)
            })
            .tooltip_show_delay(TOOLTIP_DELAY);
        // Side columns of equal width keep the launchers centred in the window.
        let side = || h_flex().w(px(LAUNCHER_SIZE)).flex_shrink_0().items_center();
        h_flex()
            .h(px(TOOLBAR_HEIGHT))
            .flex_shrink_0()
            // A line under it, like the title bar's.
            .border_b_1()
            .border_color(cx.theme().border)
            .px(px(GRID_MARGIN_X))
            .items_center()
            .child(side().child(theme))
            .child(h_flex().flex_1().justify_center().child(self.launcher_row(cx)))
            .child(side().justify_end().child(add))
            .into_any_element()
    }
    /// Applies the chosen theme (for "same as Windows", whichever Windows is using).
    fn apply_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        Theme::change(theme_mode(self.settings.theme, window.appearance()), Some(window), cx);
        finish_theme(cx);
    }

    /// The theme button: same as Windows, then light, then dark, then back.
    fn cycle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.theme = self.settings.theme.next();
        self.settings.save();
        self.apply_theme(window, cx);
        cx.notify();
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

    /// Window sizing: a minimum width, and widths that snap to whole columns.
    fn update_min_width(&mut self, window: &Window, cx: &mut Context<Self>) {
        let scale = window.scale_factor();
        if scale != self.icon_scale {
            // A different display scale: make the icons again at the new size.
            self.icon_scale = scale;
            self.sized_icons.clear();
            self.size_icons(cx);
        }
        if self.hwnd.is_none() {
            if let Ok(RawWindowHandle::Win32(handle)) = HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
                let hwnd = handle.hwnd.get();
                crate::window_size::install(hwnd);
                self.hwnd = Some(hwnd);
                if let Some(restore) = self.restore.take() {
                    crate::window_size::show_behind_foreground(
                        hwnd,
                        (restore.x, restore.y, restore.width, restore.height),
                        restore.state == WindowState::Maximized,
                        restore.state == WindowState::Minimized,
                    );
                }
            }
        }
        if let Some(hwnd) = self.hwnd {
            crate::window_size::set_snap(GRID_MARGIN_X * 2., TILE_WIDTH + GRID_GAP, GRID_GAP);
            crate::window_size::set_min(hwnd, MIN_WINDOW_WIDTH);
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
                .bg(with_alpha(gpui_kit::black(), if theme.mode.is_dark() { 0.55 } else { 0.25 }))
                .child(
                    glass_card(cx)
                        .w(px(540.))
                        .p(px(28.))
                        .flex()
                        .flex_col()
                        .gap_4()
                        .child(div().text_2xl().font_bold().child("Add a game"))
                        .child(body)
                        .child(
                            h_flex()
                                .pt_2()
                                .gap_2()
                                .child(
                                    pill_button("add-browse", "Browse for a game…", PillKind::Secondary, false, cx)
                                        .on_click(cx.listener(|app, _, _, cx| app.browse_for_games(cx))),
                                )
                                .child(div().flex_1())
                                .child(
                                    pill_button("add-cancel", "Cancel", PillKind::Quiet, false, cx)
                                        .on_click(cx.listener(|app, _, _, cx| app.close_add_panel(cx))),
                                )
                                .child(
                                    pill_button("add-selected", "Add selected", PillKind::Primary, !can_add, cx)
                                        .when(can_add, |b| {
                                            b.on_click(cx.listener(|app, _, _, cx| app.add_selected(cx)))
                                        }),
                                ),
                        ),
                ),
        )
        .with_priority(4)
        .into_any_element()
    }
    /// A launcher's (or chat app's) button: its icon; clicking opens it.
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
            .child(launcher_glyph(
                name,
                launcher.icon.as_ref().map(|icon| self.sized_icons.get(icon).unwrap_or(icon).clone()),
                theme.muted_foreground,
            ))
            .tooltip(move |window, cx| Tooltip::new(format!("Open {name}")).build(window, cx))
            .tooltip_show_delay(TOOLTIP_DELAY)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| run_action(&open, name, window, cx))
            .into_any_element()
    }

    /// The launchers (most games first, ties A to Z), then chat apps, in a
    /// centred row at the top of the game list.
    fn launcher_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let launchers = self.launchers.iter().enumerate().map(|(ix, l)| self.launcher_icon(("launcher", ix), l, cx));
        let launchers: Vec<AnyElement> = launchers.collect();
        let social: Vec<AnyElement> =
            self.social.iter().enumerate().map(|(ix, app)| self.launcher_icon(("social", ix), app, cx)).collect();
        h_flex()
            .justify_center()
            .gap_1()
            .children(launchers)
            .when(!social.is_empty(), |row| row.child(div().w(px(LAUNCHER_SIZE / 2.))))
            .children(social)
            .into_any_element()
    }

    fn tile(&self, ix: usize, game: &Game, layout: &GridLayout, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (width, height) = (layout.tile_width, layout.tile_width * COVER_ASPECT);

        let clicked = game.clone();
        let hovered = self.hovered_tile == Some(ix);
        // Its buttons show over the poster the moment it's hovered.
        let actions = hovered.then(|| self.poster_actions(game, (width, height), cx));

        let theme = cx.theme();
        let poster = div().absolute().top_0().left_0().size_full().rounded(px(TILE_RADIUS)).bg(theme.muted).child(
            if self.art_pending.contains(&game.id) && !matches!(game.art, Some(Art::Cover(_))) {
                // Still looking for its poster: a spinner, not a stand-in.
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new().large().color(theme.muted_foreground))
                    .into_any_element()
            } else {
                artwork(game, width)
            },
        );

        // The full name shows on hover only when it's cut short.
        // How far a hovered, cut-short name slides to show its end.
        let overflow = hovered
            .then(|| text_width(&game.name, window, cx) - width)
            .filter(|o| *o > 0.)
            .map(|o| o + MARQUEE_END_ROOM);
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
                    cx.notify();
                })
            })
            // A double-click anywhere on the tile plays, as well as the Play button.
            .on_click(cx.listener(move |app, event: &ClickEvent, window, cx| {
                // The press on a poster button already acted.
                if app.action_pressed.take().is_some_and(|at| at.elapsed() < BUTTON_PRESS_WINDOW) {
                    return;
                }
                if event.click_count() >= 2 {
                    app.play(&clicked, window, cx);
                }
            }))
            .child(
                div()
                    .relative()
                    .w(px(width))
                    .h(px(height))
                    .overflow_hidden()
                    .rounded(px(TILE_RADIUS))
                    .bg(theme.muted)
                    .child(poster),
            )
            // The buttons sit above the poster, not clipped to its edges.
            .children(actions.map(|actions| {
                deferred(div().absolute().top_0().left_0().w(px(width)).h(px(height)).child(actions)).with_priority(1)
            }))
            .child({
                // One line, cut short with "…"; while hovered, a name that's cut
                // short slides along to show the rest, and back.
                // Centred under the poster.
                let line = div().h(px(NAME_LINE_HEIGHT)).mt_2().text_sm().font_medium().overflow_hidden();
                match overflow {
                    Some(overflow) => line.child(div().whitespace_nowrap().child(game.name.clone()).with_animation(
                        ElementId::Name(format!("marquee-{}", game.id).into()),
                        marquee(overflow),
                        move |text, t| text.ml(px(-overflow * marquee_offset(t))),
                    )),
                    None => line.child(div().truncate().text_center().child(game.name.clone())),
                }
            })
            .h(px(height + LABEL_HEIGHT))
            .into_any_element()
    }
}

/// A click on a tile this soon after one of its buttons was pressed belongs to the button.
const BUTTON_PRESS_WINDOW: std::time::Duration = std::time::Duration::from_millis(800);

/// Corner rounding for posters, panels and cards.
const TILE_RADIUS: f32 = 8.;

/// A game name's line under its tile (text_sm).
const NAME_LINE_HEIGHT: f32 = 20.;
/// How fast a cut-short name slides (px per second), and how long it rests at each end.
const MARQUEE_SPEED: f32 = 45.;
const MARQUEE_REST: f32 = 0.9;
/// Slack at the end of a slide, so the last letter isn't clipped.
const MARQUEE_END_ROOM: f32 = 4.;

/// One round of a sliding name: rest, slide to the end, rest, slide back.
fn marquee(overflow: f32) -> Animation {
    let slide = (overflow / MARQUEE_SPEED).max(0.6);
    Animation::new(std::time::Duration::from_secs_f32(2. * (slide + MARQUEE_REST))).repeat()
}

/// How far along a sliding name is (0 start, 1 end) at `t` through a round.
fn marquee_offset(t: f32) -> f32 {
    let ease = |x: f32| x * x * (3. - 2. * x);
    match t {
        t if t < 0.2 => 0.,
        t if t < 0.5 => ease((t - 0.2) / 0.3),
        t if t < 0.7 => 1.,
        t => 1. - ease((t - 0.7) / 0.3),
    }
}
/// Labels for the recency groups (see `recency_groups`).
const GROUP_NAMES: [&str; 3] = ["Played in the last week", "Played in the last month", "Everything else"];

/// How much of a tile's width an icon (rather than cover art) takes up.
const ICON_SHARE: f32 = 0.6;
/// Glass buttons over a hovered poster: sizes, and distance from its edges.
const GLASS_PLAY_BUTTON: f32 = 96.;
/// The button below Play: always two thirds its size.
const GLASS_BUTTON: f32 = GLASS_PLAY_BUTTON * 2. / 3.;
const GLASS_INSET: f32 = 14.;
/// Space above and below the grid.
const GRID_PADDING: f32 = 20.;
/// Least space either side of the grid (the window snaps to widths where it's exactly this).
const GRID_MARGIN_X: f32 = 24.;
/// The title bar's height.
const TITLE_BAR_HEIGHT: f32 = 34.;
/// The window is never narrower than this.
const MIN_WINDOW_WIDTH: f32 = 480.;
/// Minimise, maximise and close, on the right of the title bar, less its left padding.
const WINDOW_CONTROLS_WIDTH: f32 = 3. * 34. - 12.;

/// The app's own touches on the theme: the title bar shows the window's background.
fn finish_theme(cx: &mut App) {
    Theme::global_mut(cx).colors.title_bar = gpui_kit::transparent_black();
}

/// color at exactly lpha (gpui's opacity multiplies the existing alpha instead).
fn with_alpha(color: Hsla, alpha: f32) -> Hsla {
    Hsla { a: alpha, ..color }
}

/// Light or dark: as chosen, or following Windows.
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
        // Fills the tile: scaled evenly until it covers it, trimming what spills
        // over; never stretched.
        Some(Art::Cover(path)) => {
            img(path.clone()).size_full().rounded(px(TILE_RADIUS)).object_fit(ObjectFit::Cover).into_any_element()
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

/// A toolbar button the size of the launcher buttons, with an icon.
fn bar_button(id: &'static str, icon: Icon, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    div()
        .id(id)
        .size(px(LAUNCHER_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .text_color(theme.foreground)
        .hover(|style| style.bg(theme.list_hover))
        .child(icon.size(px(BAR_ICON)))
}
/// A toolbar button's icon size.
const BAR_ICON: f32 = 20.;

/// The icon (or initials) for a launcher button.
fn launcher_glyph(name: &str, icon: Option<PathBuf>, muted: Hsla) -> AnyElement {
    match icon {
        Some(path) => img(path).size(px(LAUNCHER_ICON)).object_fit(ObjectFit::Contain).into_any_element(),
        None => div()
            .text_xs()
            .font_semibold()
            .text_color(muted)
            .child(name.chars().filter(|c| c.is_uppercase()).take(2).collect::<String>())
            .into_any_element(),
    }
}
/// A launcher button's size, and its icon's.
const LAUNCHER_SIZE: f32 = 36.;
const LAUNCHER_ICON: f32 = 26.;
/// The bar under the title bar.
const TOOLBAR_HEIGHT: f32 = 48.;

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

/// A round button over a poster, in the theme's style: white with a dark icon
/// in light mode, charcoal with a white icon in dark mode. `danger` makes it
/// red (a remove waiting to be confirmed). Solid, and it changes colour the
/// instant it's hovered or pressed: nothing animates.
fn glass_button(id: impl Into<ElementId>, content: AnyElement, size: f32, danger: bool, dark: bool) -> Stateful<Div> {
    let shade = |l: f32| gpui_kit::hsla(240. / 360., 0.06, l, 1.);
    let (fill, hover, pressed, ink, edge) = if danger {
        let alarm = |l: f32| gpui_kit::hsla(0., 0.72, l, 1.);
        (alarm(0.5), alarm(0.44), alarm(0.38), gpui_kit::white(), alarm(0.6))
    } else if dark {
        (shade(0.16), shade(0.24), shade(0.3), gpui_kit::white(), shade(0.36))
    } else {
        (gpui_kit::white(), shade(0.93), shade(0.86), shade(0.15), shade(0.85))
    };
    div()
        .id(id)
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(fill)
        .border_1()
        .border_color(edge)
        .shadow(vec![BoxShadow {
            color: gpui_kit::black().opacity(0.35),
            offset: point(px(0.), px(4.)),
            blur_radius: px(12.),
            spread_radius: px(0.),
            inset: false,
        }])
        .text_color(ink)
        .hover(move |style| style.bg(hover))
        .active(move |style| style.bg(pressed))
        .child(content)
}

/// A button's slot (only the button itself takes the mouse).
fn glass_slot(size: f32, button: impl IntoElement) -> Div {
    div().size(px(size)).flex().items_center().justify_center().child(button)
}
/// An icon sized for a glass button.
fn icon_content(icon: IconName, size: f32) -> AnyElement {
    Icon::new(icon).size(px(size * ICON_FILL)).into_any_element()
}

/// A launcher button's size when a game has several, side by side.
const GLASS_SMALL_BUTTON: f32 = GLASS_BUTTON * 0.72;
/// How much of a glass button its icon fills, and a launcher glyph (tight-cropped, so a touch less).
const ICON_FILL: f32 = 0.54;
const GLYPH_FILL: f32 = 0.5;

/// Glyph ink: white on dark glass, near-black on light glass (as the buttons' icons).
const GLYPH_ON_DARK: [u8; 3] = [255, 255, 255];
const GLYPH_ON_LIGHT: [u8; 3] = [40, 40, 40];

/// Adds a quick tooltip (after `TOOLTIP_DELAY`).
fn with_tooltip(element: Stateful<Div>, text: impl Into<SharedString>) -> Stateful<Div> {
    let text: SharedString = text.into();
    element.tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx)).tooltip_show_delay(TOOLTIP_DELAY)
}

/// A full-width option: a checkbox beside a title (and optional detail lines
/// under it). The whole row is clickable and highlights on hover.
/// A frosted glass card for panels: generously rounded, with light catching
/// its top edge and a deep, soft shadow so it floats above the window.
fn glass_card(cx: &App) -> Div {
    let theme = cx.theme();
    let dark = theme.mode.is_dark();
    div()
        .relative()
        .rounded(px(CARD_RADIUS))
        .bg(theme.popover)
        .border_1()
        .border_color(if dark { gpui_kit::white().opacity(0.12) } else { gpui_kit::white().opacity(0.7) })
        .shadow(vec![
            BoxShadow {
                color: gpui_kit::black().opacity(if dark { 0.55 } else { 0.22 }),
                offset: point(px(0.), px(24.)),
                blur_radius: px(48.),
                spread_radius: px(-8.),
                inset: false,
            },
            BoxShadow {
                color: gpui_kit::white().opacity(if dark { 0.08 } else { 0.6 }),
                offset: point(px(0.), px(1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            },
        ])
}

#[derive(Clone, Copy, PartialEq)]
enum PillKind {
    /// The main action: a colourful gradient.
    Primary,
    /// A secondary action: frosted glass.
    Secondary,
    /// A quiet action: text until hovered.
    Quiet,
}

/// A rounded pill button, in the app's style.
fn pill_button(
    id: &'static str,
    label: impl Into<SharedString>,
    kind: PillKind,
    disabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let dark = theme.mode.is_dark();
    let glass = if dark { gpui_kit::white().opacity(0.1) } else { gpui_kit::black().opacity(0.05) };
    let glass_hover = if dark { gpui_kit::white().opacity(0.18) } else { gpui_kit::black().opacity(0.1) };
    let pill = div()
        .id(id)
        .h(px(PILL_HEIGHT))
        .px(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .gap_2()
        .rounded_full()
        .text_sm()
        .font_semibold()
        .child(label.into());
    let pill = match kind {
        PillKind::Primary => pill
            .text_color(gpui_kit::white())
            .bg(linear_gradient(120., linear_color_stop(ACCENT_FROM, 0.), linear_color_stop(ACCENT_TO, 1.)))
            .shadow(vec![BoxShadow {
                color: with_alpha(ACCENT_TO, 0.45),
                offset: point(px(0.), px(6.)),
                blur_radius: px(16.),
                spread_radius: px(-4.),
                inset: false,
            }])
            .when(!disabled, |pill| pill.hover(|s| s.opacity(0.92))),
        PillKind::Secondary => pill.bg(glass).border_1().border_color(theme.border).hover(move |s| s.bg(glass_hover)),
        PillKind::Quiet => pill.text_color(theme.muted_foreground).hover(move |s| s.bg(glass)),
    };
    pill.when(disabled, |pill| pill.opacity(0.4))
}

/// The accent gradient, for primary actions.
const ACCENT_FROM: Hsla = Hsla { h: 222. / 360., s: 0.95, l: 0.62, a: 1. };
const ACCENT_TO: Hsla = Hsla { h: 268. / 360., s: 0.85, l: 0.64, a: 1. };
/// Panels: corner rounding, entrance, and pill buttons' height.
const CARD_RADIUS: f32 = 20.;
const PILL_HEIGHT: f32 = 36.;

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
        .py_2()
        .rounded(px(12.))
        .hover(move |style| style.bg(hover))
        .child(Checkbox::new("check").checked(checked).large())
        .child(
            v_flex()
                .min_w_0()
                .child(div().text_sm().font_medium().child(title.into()))
                .children(detail.map(|detail| div().text_xs().text_color(muted).child(detail))),
        )
}

/// Where the library is kept between runs, so it shows the moment the app opens.
fn library_file() -> Option<PathBuf> {
    Some(mulch_core::paths::data_dir()?.join("library.json"))
}

/// The library as last seen, if it was saved.
fn load_library() -> Option<Vec<Game>> {
    let text = std::fs::read_to_string(library_file()?).ok()?;
    serde_json::from_str(&text).ok()
}

/// Saves the library (with its art), off the UI thread.
fn save_library(games: Vec<Game>) {
    std::thread::spawn(move || {
        let Some(path) = library_file() else { return };
        if let (Some(dir), Ok(text)) = (path.parent(), serde_json::to_string(&games)) {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(path, text);
        }
    });
}

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        mulch_launcher::trace::mark("render");
        self.update_min_width(window, cx);
        let viewport = window.viewport_size();
        let layout = grid_layout(&self.recency_groups(), f32::from(viewport.width) - GRID_MARGIN_X * 2.);
        let title_bar = self.title_bar(cx);
        let toolbar = self.toolbar(cx);
        // Rows built explicitly (rather than by wrapping) so each group starts a new row.
        let mut tiles: Vec<AnyElement> =
            self.games.iter().enumerate().map(|(ix, game)| self.tile(ix, game, &layout, window, cx)).collect();
        let mut sections = Vec::new();
        for section in &layout.sections {
            let mut rows = Vec::new();
            for &size in &section.rows {
                let rest = tiles.split_off(size.min(tiles.len()));
                rows.push(
                    h_flex()
                        .items_start()
                        .gap(px(GRID_GAP))
                        .justify_center()
                        .children(std::mem::replace(&mut tiles, rest)),
                );
            }
            // Each row and heading is centred.
            sections.push(
                v_flex()
                    .gap(px(GRID_GAP))
                    .when(layout.labelled, |this| {
                        this.child(
                            div()
                                .h(px(HEADING_HEIGHT - GRID_GAP))
                                .flex()
                                .items_end()
                                .justify_center()
                                .text_xl()
                                .font_bold()
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
                .pt(px(GRID_PADDING))
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
