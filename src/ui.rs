//! The main window: a slim title bar, a toolbar (chat apps and launchers on
//! the left, buttons on the right) and every detected game as a tile, grouped
//! by when it was last played and sized so they all fit if they can.

use gpui_kit::component::button::*;
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
use mulch_manual as manual;
use mulch_posters as posters;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

const APP_NAME: &str = "MulchLauncher";
const REPO_URL: &str = "https://github.com/apotenza92/mulch-launcher";

/// `restore`: set when restarting after an update, to reopen where the old
/// copy was, in the background.
pub fn run(restore: Option<Restore>) {
    gpui_kit::application().with_assets(crate::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(theme_mode(cx.window_appearance()), None, cx);
        make_glassy(cx);

        let options = WindowOptions {
            // Our own slim title bar (see `title_bar`), with the toolbar under
            // it. The title is still set for the taskbar and Alt+Tab.
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            // Frosted glass: the desktop behind shows through, blurred.
            window_background: WindowBackgroundAppearance::Blurred,
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
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| MulchApp::new(restore, window, cx)))
            .expect("failed to open the window");
    });
}

/// The one window shown when the downloaded copy is run: an "Add to desktop"
/// checkbox (on by default) and Install. Closing it installs nothing.
pub fn run_installer() {
    gpui_kit::application().with_assets(crate::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(theme_mode(cx.window_appearance()), None, cx);
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
            .child(card_entrance(
                "installer-body",
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
            ))
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
    /// Each poster button's height (0 resting, 1 hovered, 2 pressed), springing between them.
    button_heights: HashMap<String, Tween>,
    /// Games being removed, fading out before they go.
    leaving: HashMap<String, Tween>,
    /// Each launcher's icon as a one-colour glyph (light ink, dark ink), for poster buttons.
    glyphs: HashMap<Platform, (PathBuf, PathBuf)>,
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
        let mut app = Self {
            games: Vec::new(),
            launchers: Vec::new(),
            social: Vec::new(),
            add_panel: None,
            hwnd: None,
            hovered_tile: None,
            tilt: (0., 0.),
            hovered_bounds: Rc::default(),
            scanning: false,
            confirm_remove: None,
            lifts: HashMap::new(),
            restore,
            update_ready: false,
            active: true,
            checking: false,
            art_pending: HashSet::new(),
            button_heights: HashMap::new(),
            leaving: HashMap::new(),
            accents: HashMap::new(),
            glyphs: HashMap::new(),
            theme_fade: None,
            first_shown: None,
            action_clicked: false,
            history: History::load(),
            // Coming back to the window (e.g. after playing): re-sort so the
            // game just played is first.
            _subscriptions: vec![
                // Follow Windows switching between light and dark.
                cx.observe_window_appearance(window, |app, window, cx| app.fade_theme(window, cx)),
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
        app.rescan(cx);
        app.watch_for_running_games(cx);
        app.watch_for_library_changes(cx);
        app.watch_for_updates(window, cx);
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
            let found = cx.background_spawn(async move { library_signature(&scan::scan_all(&[]).games) }).await;
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
            this.update(cx, |app, cx| {
                app.art_pending.clear();
                app.apply_art(games, launchers, cx);
            })
            .ok();
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
        // Only games with nothing to show yet (new ones) wait for their art;
        // everything already showing stays exactly as it is.
        self.art_pending = self.games.iter().filter(|g| g.art.is_none()).map(|g| g.id.clone()).collect();
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

    /// A glass button whose height springs between resting, hovered and
    /// pressed as the mouse moves over it and presses it.
    #[allow(clippy::too_many_arguments)]
    fn glass(
        &self,
        game_id: &str,
        which: &'static str,
        content: AnyElement,
        size: f32,
        danger: bool,
        dark: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let key = format!("{game_id}/{which}");
        let height = Tween::value(&self.button_heights, &key);
        let (on_hover, on_down, on_up) = (key.clone(), key.clone(), key);
        glass_button(which, content, size, danger, dark, height)
            .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                Tween::spring_to(&mut app.button_heights, &on_hover, if *hovered { 1. } else { 0. }, BUTTON_SPRING);
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _, _, cx| {
                    Tween::spring_to(&mut app.button_heights, &on_down, 2., BUTTON_PRESS_SPRING);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |app, _, _, cx| {
                    Tween::spring_to(&mut app.button_heights, &on_up, 1., BUTTON_SPRING);
                    cx.notify();
                }),
            )
    }

    /// Glass buttons over a hovered poster: Play, big, centred; below it one
    /// smaller button: Show in its launcher, or for games the user added,
    /// Remove (which asks again first).
    fn poster_actions(&self, game: &Game, size: (f32, f32), cx: &mut Context<Self>) -> Div {
        let dark = cx.theme().mode.is_dark();
        let mut action: Option<AnyElement> = None;
        if let Some(show) = game.show_in_launcher.clone() {
            let launcher = match game.platform {
                Platform::Xbox => "Microsoft Store",
                Platform::Gog => "GOG Galaxy",
                other => other.label(),
            };
            let name = game.name.clone();
            // The launcher's own logo, as a glyph in the button's ink.
            let content = match self.glyphs.get(&game.platform) {
                Some((on_dark, on_light)) => img(if dark { on_dark.clone() } else { on_light.clone() })
                    .size(px(GLASS_BUTTON * GLYPH_FILL))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => Icon::new(IconName::ExternalLink).size(px(GLASS_BUTTON * ICON_FILL)).into_any_element(),
            };
            let button = self.glass(&game.id, "show-launcher", content, GLASS_BUTTON, false, dark, cx).on_click(
                cx.listener(move |app, _, window, cx| {
                    app.action_clicked = true;
                    run_action(&show, &name, window, cx);
                }),
            );
            action = Some(with_tooltip(button, format!("Show in {launcher}")).into_any_element());
        } else if let (Platform::Manual, Action::Exe { path, .. }) = (game.platform, &game.launch) {
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
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.action_clicked = true;
                    if app.confirm_remove.as_deref() == Some(id.as_str()) {
                        app.confirm_remove = None;
                        app.remove_manual(exe.clone(), cx);
                    } else {
                        app.confirm_remove = Some(id.clone());
                        cx.notify();
                    }
                }));
            let tip = if confirming { "Click again to remove" } else { "Remove" };
            action = Some(with_tooltip(button, tip).into_any_element());
        }

        // Play, big, in the middle of the space above the bottom row.
        let (width, height) = size;
        let half = (GLASS_PLAY_BUTTON + GLASS_PRESS_GROW) / 2.;
        let row = GLASS_INSET + GLASS_BUTTON + GLASS_PRESS_GROW;
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
            .on_click(cx.listener(move |app, _, window, cx| {
                app.action_clicked = true;
                app.play(&g, window, cx);
            }));
        let play = with_tooltip(play, "Play").absolute().left(px(x - half)).top(px(y - half));

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(play)
            .child(h_flex().absolute().left_0().right_0().bottom(px(GLASS_INSET)).justify_center().children(action))
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
        self.make_glyphs(cx);
        cx.notify();
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

    /// Add a game, in the title bar's left corner. (The library re-checks
    /// itself, and the theme follows Windows.) Drawn on a layer above the title
    /// bar, which claims its own clicks (the title bar is the drag area).
    fn title_buttons(&self, cx: &mut Context<Self>) -> AnyElement {
        let add = quick_tooltip(
            "add-game-tip",
            "Add game manually",
            Button::new("add-game")
                .ghost()
                .small()
                .icon(IconName::Plus)
                .on_click(cx.listener(|app, _, _, cx| app.open_add_panel(cx))),
        );
        // The source on GitHub, beside the window controls on the right.
        let github = quick_tooltip(
            "github-tip",
            "MulchLauncher on GitHub",
            Button::new("github")
                .ghost()
                .small()
                .icon(Icon::empty().path("mulch/github.svg"))
                .on_click(|_, _, cx| cx.open_url(REPO_URL)),
        );
        let github = deferred(
            h_flex()
                .id("title-github")
                .occlude()
                .absolute()
                .top_0()
                .right(px(WINDOW_CONTROLS_WIDTH + 12. + 4.))
                .h(px(TITLE_BAR_HEIGHT))
                .items_center()
                .child(github),
        );
        let left = deferred(
            h_flex()
                .id("title-buttons")
                .occlude()
                .absolute()
                .top_0()
                .left(px(TITLE_BUTTONS_INSET))
                .h(px(TITLE_BAR_HEIGHT))
                .items_center()
                .child(add),
        );
        // A full-width layer, so the GitHub button can sit against the right edge.
        div().absolute().top_0().left_0().w_full().h(px(TITLE_BAR_HEIGHT)).child(left).child(github).into_any_element()
    }
    /// Applies Windows' light or dark mode.
    fn apply_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        Theme::change(theme_mode(window.appearance()), Some(window), cx);
        make_glassy(cx);
    }

    /// Switches to Windows' current light or dark mode, blending the colours over a moment.
    fn fade_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let from = self
            .theme_fade
            .as_ref()
            .map_or_else(|| cx.theme().colors, |(from, to, fade)| blend_colors(from, to, fade.now()));
        self.apply_theme(window, cx);
        let to = cx.theme().colors;
        let fade = Tween { from: 0., to: 1., since: std::time::Instant::now(), duration: THEME_FADE, spring: false };
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
    fn update_min_width(&mut self, window: &Window) {
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
                .with_animation("add-backdrop-fade", Animation::new(CARD_ENTRANCE), |b, t| b.opacity(t.min(1.) * 1.))
                .child(card_entrance(
                    "add-card",
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
                )),
        )
        .with_priority(4)
        .into_any_element()
    }
    fn tile(&self, ix: usize, game: &Game, layout: &GridLayout, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let (width, height) = (layout.tile_width, layout.tile_width * COVER_ASPECT);

        let clicked = game.clone();
        let lift = Tween::value(&self.lifts, &game.id);
        // Glass buttons fade in over the poster while it's hovered.
        let actions = (lift > 0.).then(|| self.poster_actions(game, (width, height), cx).opacity(lift));

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
        let hovered = self.hovered_tile == Some(ix);
        let (dx, dy) = if hovered { self.tilt } else { (0., 0.) };
        // How far a hovered, cut-short name slides to show its end.
        let overflow = hovered
            .then(|| text_width(&game.name, window, cx) - width)
            .filter(|o| *o > 0.)
            .map(|o| o + MARQUEE_END_ROOM);
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
                    Tween::spring_to(&mut app.lifts, &id, if *hovered { 1. } else { 0. }, LIFT_DURATION);
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
            // The buttons float on their own plane above the poster: not clipped
            // to its edges, and shifting further than it with the tilt.
            .children(actions.map(|actions| {
                deferred(
                    div()
                        .absolute()
                        .left(px(TILT_SHIFT * BUTTON_PARALLAX * dx * lift))
                        .top(px((-LIFT + TILT_SHIFT * BUTTON_PARALLAX * dy) * lift))
                        .w(px(width))
                        .h(px(height))
                        .child(actions),
                )
                .with_priority(1)
            }))
            .child({
                // One line, cut short with "…"; while hovered, a name that's cut
                // short slides along to show the rest, and back.
                let line = div().h(px(NAME_LINE_HEIGHT)).mt_2().text_sm().font_medium().overflow_hidden();
                match overflow {
                    Some(overflow) => line.child(div().whitespace_nowrap().child(game.name.clone()).with_animation(
                        ElementId::Name(format!("marquee-{}", game.id).into()),
                        marquee(overflow),
                        move |text, t| text.ml(px(-overflow * marquee_offset(t))),
                    )),
                    None => line.child(div().truncate().child(game.name.clone())),
                }
            })
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

/// A value easing from where it was toward a target (0 to 1).
#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    since: std::time::Instant,
    duration: std::time::Duration,
    /// Springy (a gentle overshoot as it settles, like Apple's UI) rather than a plain ease-out.
    spring: bool,
}

impl Tween {
    fn now(&self) -> f32 {
        let t = (self.since.elapsed().as_secs_f32() / self.duration.as_secs_f32()).min(1.);
        let eased = if self.spring { spring(t) } else { ease_out_quint()(t) };
        self.from + (self.to - self.from) * eased
    }

    /// Starts springing `id` toward `to` from wherever it is now.
    fn spring_to(tweens: &mut HashMap<String, Tween>, id: &str, to: f32, duration: std::time::Duration) {
        let from = Tween::value(tweens, id);
        tweens.insert(id.to_string(), Tween { from, to, since: std::time::Instant::now(), duration, spring: true });
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
        tweens.insert(id.to_string(), Tween { from, to, since: std::time::Instant::now(), duration, spring: false });
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

/// A critically-damped-ish spring from 0 to 1 over `t` in 0..1: quick to
/// start, overshooting by about 6% and settling, as Apple's animations do.
fn spring(t: f32) -> f32 {
    if t >= 1. { 1. } else { 1. - (-7. * t).exp() * (8. * t).cos() }
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
const LIFT_DURATION: std::time::Duration = std::time::Duration::from_millis(520);
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
/// How far the title bar's buttons sit from the window's left edge.
const TITLE_BUTTONS_INSET: f32 = 8.;
/// The window is never narrower than this.
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
/// Light or dark, following Windows.
fn theme_mode(windows: WindowAppearance) -> ThemeMode {
    match windows {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
        _ => ThemeMode::Light,
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
    let height =
        TITLE_BAR_HEIGHT + GRID_PADDING + GROUPS * HEADING_HEIGHT + (GROUPS - 0.5) * row + (GROUPS - 1.) * GRID_GAP;
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

/// A round glass button over a poster, in the theme's style: frosted white
/// with a dark icon in light mode, dark glass with a white icon in dark mode.
/// `danger` tints it red (a remove waiting to be confirmed). `height` is how
/// raised it is: 0 resting just above the poster, 1 hovered, 2 pressed (and
/// anything in between, as it springs). Its slot stays the pressed size, so
/// nothing around it moves.
fn glass_button(
    id: &'static str,
    content: AnyElement,
    size: f32,
    danger: bool,
    dark: bool,
    height: f32,
) -> Stateful<Div> {
    let (glass, ink, edge) = if dark {
        // Frosted charcoal rather than clear black.
        (gpui_kit::hsla(240. / 360., 0.06, 0.16, 1.), gpui_kit::white(), gpui_kit::white().opacity(0.4))
    } else {
        (gpui_kit::white(), gpui_kit::black().opacity(0.8), gpui_kit::black().opacity(0.12))
    };
    let (fill, hover, ink, glow) = if danger {
        (gpui_kit::red().opacity(0.75), gpui_kit::red().opacity(0.95), gpui_kit::white(), gpui_kit::red().opacity(0.7))
    } else if dark {
        (glass.opacity(0.72), glass.opacity(0.88), ink, gpui_kit::white().opacity(0.45))
    } else {
        (glass.opacity(0.7), glass.opacity(0.95), ink, gpui_kit::white().opacity(0.9))
    };
    let raised = height.clamp(0., 1.);
    let pressed = (height - 1.).max(0.);
    let grow = GLASS_GROW * height.min(1.) + (GLASS_PRESS_GROW - GLASS_GROW) * pressed;
    let lift = GLASS_LIFT * height.min(1.) + (GLASS_PRESS_LIFT - GLASS_LIFT) * pressed;
    div().id(id).size(px(size + GLASS_PRESS_GROW)).flex().items_center().justify_center().child(
        div()
            .relative()
            .size(px(size + grow))
            .top(px(-lift))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(mix(fill, hover, raised))
            .border_1()
            .border_color(mix(edge, gpui_kit::white().opacity(0.75), raised))
            .shadow(bevel(height, glow))
            .text_color(ink)
            // Glassy: light catching the top of the dome.
            .child(div().absolute().top_0().left_0().size_full().rounded_full().bg(linear_gradient(
                180.,
                linear_color_stop(gpui_kit::white().opacity(0.28), 0.),
                linear_color_stop(gpui_kit::white().opacity(0.), 0.55),
            )))
            .child(content),
    )
}

/// An icon sized for a glass button.
fn icon_content(icon: IconName, size: f32) -> AnyElement {
    Icon::new(icon).size(px(size * ICON_FILL)).into_any_element()
}

/// Colour part-way from `a` to `b` (`t` from 0 to 1, a little past for springs).
fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    let m = |x: f32, y: f32| (x + (y - x) * t).clamp(0., 1.);
    Hsla::from(Rgba { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: m(a.a, b.a) })
}
/// How much of a glass button its icon fills, and a launcher glyph (tight-cropped, so a touch less).
const ICON_FILL: f32 = 0.54;
/// How much a glass button grows when hovered.
const GLASS_GROW: f32 = 12.;
/// How far a hovered glass button rises.
const GLASS_LIFT: f32 = 6.;
/// Pressed, it rises further still.
const GLASS_PRESS_LIFT: f32 = 12.;
const GLASS_PRESS_GROW: f32 = 18.;
/// How long a button takes to spring to hovered or resting, and to pressed.
const BUTTON_SPRING: std::time::Duration = std::time::Duration::from_millis(480);
const BUTTON_PRESS_SPRING: std::time::Duration = std::time::Duration::from_millis(340);
/// The buttons' layer moves this much more than the poster as it tilts (parallax).
const BUTTON_PARALLAX: f32 = 2.5;
const GLYPH_FILL: f32 = 0.5;

/// Glyph ink: white on dark glass, near-black on light glass (as the buttons' icons).
const GLYPH_ON_DARK: [u8; 3] = [255, 255, 255];
const GLYPH_ON_LIGHT: [u8; 3] = [40, 40, 40];

/// A glass button's shadows by height (0 resting just above the poster,
/// 1 hovered, 2 pressed, blended in between): a domed bevel (bright top
/// edge, darker bottom) and a drop shadow that falls further and softer the
/// higher it is, plus a glow as it rises.
fn bevel(height: f32, glow: Hsla) -> Vec<BoxShadow> {
    let shadow = |color: Hsla, y: f32, blur: f32, spread: f32, inset: bool| BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset,
    };
    // Per height: far shadow (offset, blur, opacity), near shadow, top light, bottom shade.
    const LEVELS: [[f32; 8]; 3] = [
        [8., 14., 0.4, 2., 4., 0.25, 0.4, 0.18],
        [16., 24., 0.5, 5., 8., 0.3, 0.6, 0.28],
        [26., 34., 0.55, 8., 12., 0.3, 0.7, 0.32],
    ];
    let h = height.clamp(0., 2.2);
    let (lo, t) = if h <= 1. { (0, h) } else { (1, (h - 1.).min(1.2)) };
    let v: Vec<f32> = (0..8).map(|i| LEVELS[lo][i] + (LEVELS[lo + 1][i] - LEVELS[lo][i]) * t).collect();
    let (light, dark) = (gpui_kit::white(), gpui_kit::black());
    vec![
        shadow(dark.opacity(v[2]), v[0], v[1], -2., false),
        shadow(dark.opacity(v[5]), v[3], v[4], 0., false),
        shadow(light.opacity(v[6]), 2., 1., 0., true),
        shadow(dark.opacity(v[7]), -3., 4., 0., true),
        shadow(glow.opacity(h.min(1.)), 0., 22., 1., false),
    ]
}
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

/// A card's entrance: it springs up into place as it fades in.
fn card_entrance<E: Styled + IntoElement + 'static>(id: &'static str, card: E) -> AnimationElement<E> {
    card.with_animation(id, Animation::new(CARD_ENTRANCE).with_easing(spring), |card, t| {
        card.opacity((t * 2.).min(1.)).top(px(CARD_RISE * (1. - t)))
    })
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
const CARD_RISE: f32 = 18.;
const CARD_ENTRANCE: std::time::Duration = std::time::Duration::from_millis(520);
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

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_min_width(window);
        self.step_theme_fade(window, cx);
        if Tween::tidy(&mut self.lifts) | Tween::tidy(&mut self.button_heights) | Tween::tidy_up(&mut self.leaving) {
            window.request_animation_frame();
        }
        let viewport = window.viewport_size();
        let layout = grid_layout(&self.recency_groups(), f32::from(viewport.width) - GRID_MARGIN_X * 2.);
        let title_bar = self.title_bar(cx);
        let title_buttons = self.title_buttons(cx);
        // Rows built explicitly (rather than by wrapping) so each group starts a new row.
        let mut tiles: Vec<AnyElement> =
            self.games.iter().enumerate().map(|(ix, game)| self.tile(ix, game, &layout, window, cx)).collect();
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
                                .flex()
                                .items_end()
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

        v_flex().relative().size_full().children(add_panel).child(title_bar).child(title_buttons).child(
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
