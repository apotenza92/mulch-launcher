//! The main window: every detected game as a tile, sized so they all fit
//! (scrolling once tiles reach their minimum size), plus a row of icons for
//! the installed launchers, in alphabetical order.

use mulch_launcher::install;
use mulch_launcher::launch;
use mulch_launcher::layout::{COVER_ASPECT, GRID_GAP, GridLayout, fit_tiles};
use mulch_art as art;
use mulch_history::{self as history, History};
use mulch_launcher::scan::{self, Action, Art, Game, Launcher, Platform, ScanResult};
use mulch_manual as manual;
use mulch_posters as posters;
use mulch_launcher::settings::Settings;
use gpui_kit::component::button::*;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{WindowExt, *};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;

const APP_NAME: &str = "MulchLauncher";

/// `pin_requested`: the installed copy was started by setup with "pin to
/// taskbar" ticked, so ask Windows to pin it.
pub fn run(pin_requested: bool) {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(move |cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..Default::default() }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1240.), px(820.)), cx))),
            window_min_size: Some(size(px(480.), px(360.))),
            app_id: Some(APP_NAME.into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| MulchApp::new(pin_requested, window, cx)))
            .expect("failed to open the window");
    });
}

struct MulchApp {
    games: Vec<Game>,
    launchers: Vec<Launcher>,
    settings: Settings,
    scanning: bool,
    /// A game the user clicked, waiting for a second click on Play.
    pending_play: Option<PendingPlay>,
    /// First-run setup, while it's showing.
    setup: Option<SetupStep>,
    pin_on_finish: bool,
    /// Launchers pinned to the user's taskbar (setup suggests unpinning them).
    pinned_launchers: Vec<String>,
    /// Ask Windows to pin us on the next render (needs a live window).
    pin_pending: bool,
    /// When each game was last played, for sorting most recent first.
    history: History,
    _subscriptions: Vec<Subscription>,
}

/// How often to look for running games (to record them as played).
const PLAY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq)]
enum SetupStep {
    Welcome,
    OtherGames,
    Taskbar,
}

struct PendingPlay {
    game: Game,
    /// Where the click happened; the Play button opens under it.
    at: Point<Pixels>,
}

impl MulchApp {
    fn new(pin_requested: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();
        let setup = (!settings.setup_done).then_some(SetupStep::Welcome);
        let mut app = Self {
            setup,
            pin_on_finish: false,
            pinned_launchers: Vec::new(),
            pin_pending: pin_requested,
            games: Vec::new(),
            launchers: Vec::new(),
            settings,
            scanning: false,
            pending_play: None,
            history: History::load(),
            // Coming back to the window (e.g. after playing): re-sort so the
            // game just played is first.
            _subscriptions: vec![cx.observe_window_activation(window, |app, window, cx| {
                if window.is_window_active() {
                    app.resort(cx);
                }
            })],
        };
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
        self.games = result.games;
        self.history.sort(&mut self.games);
        self.launchers = result.launchers;
        self.launchers.sort_by_key(|l| l.name.to_lowercase());
        let names: Vec<&str> = self.launchers.iter().map(|l| l.name).collect();
        self.pinned_launchers = install::pinned_launchers(&names);
        self.scanning = false;
        self.pending_play = None;
        cx.notify();
    }

    /// First click on a game: show a Play button right under the cursor, so a
    /// second click (or a double-click) starts it and a stray click does nothing.
    fn request_play(&mut self, game: Game, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.pending_play = Some(PendingPlay { game, at });
        cx.notify();
    }

    fn cancel_play(&mut self, cx: &mut Context<Self>) {
        self.pending_play = None;
        cx.notify();
    }

    fn play_pending(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pending) = self.pending_play.take() {
            run_action(&pending.game.launch, &pending.game.name, window, cx);
            self.history.record(&pending.game.id, history::now());
            self.history.save();
        }
        cx.notify();
    }

    fn play_card(&self, pending: &PendingPlay, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let card = v_flex()
            .id("play-card")
            .occlude()
            .w(px(PLAY_CARD_WIDTH))
            .p(px(PLAY_CARD_PADDING))
            .gap_2()
            .rounded_lg()
            .bg(theme.popover)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(
                Button::new("confirm-play")
                    .primary()
                    .w_full()
                    .h(px(PLAY_BUTTON_HEIGHT))
                    .icon(IconName::Play)
                    .label("Play")
                    .on_click(cx.listener(|app, _, window, cx| app.play_pending(window, cx))),
            )
            .child(div().text_sm().font_medium().truncate().child(pending.game.name.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("via {}", pending.game.platform.label())))
            .child(
                Button::new("cancel-play")
                    .ghost()
                    .xsmall()
                    .label("Cancel")
                    .on_click(cx.listener(|app, _, _, cx| app.cancel_play(cx))),
            );

        deferred(
            div()
                .id("play-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|app, _, _, cx| app.cancel_play(cx)))
                .on_mouse_down(MouseButton::Right, cx.listener(|app, _, _, cx| app.cancel_play(cx)))
                .child(
                    anchored()
                        .position_mode(AnchoredPositionMode::Window)
                        .position(pending.at)
                        // Put the middle of the Play button under the cursor.
                        .offset(point(
                            px(-PLAY_CARD_WIDTH / 2.),
                            px(-(PLAY_CARD_PADDING + PLAY_BUTTON_HEIGHT / 2.)),
                        ))
                        .snap_to_window_with_margin(px(8.))
                        .child(card),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    fn apply_art(&mut self, games: Vec<Game>, launchers: Vec<Launcher>, cx: &mut Context<Self>) {
        // Icons may have been added or replaced with trimmed copies.
        let game_art: HashMap<String, Art> = games.into_iter().filter_map(|g| Some((g.id, g.art?))).collect();
        for game in &mut self.games {
            if let Some(art) = game_art.get(&game.id) {
                game.art = Some(art.clone());
            }
        }
        let launcher_icons: HashMap<&str, PathBuf> = launchers.into_iter().filter_map(|l| Some((l.name, l.icon?))).collect();
        for launcher in &mut self.launchers {
            if let Some(icon) = launcher_icons.get(launcher.name) {
                launcher.icon = Some(icon.clone());
            }
        }
        cx.notify();
    }

    fn add_games(&mut self, cx: &mut Context<Self>) {
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

    fn remove_manual(&mut self, exe: PathBuf, cx: &mut Context<Self>) {
        let _ = manual::remove(&exe);
        self.rescan(cx);
    }

    fn set_setup_step(&mut self, step: SetupStep, cx: &mut Context<Self>) {
        self.setup = Some(step);
        cx.notify();
    }

    /// Ends first-run setup. A downloaded copy installs itself and hands over
    /// to the installed copy; an installed or development copy just carries on.
    fn finish_setup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings.setup_done = true;
        self.settings.save();
        self.setup = None;

        if !install::is_dev_build() && !install::is_installed_copy() {
            match install::install() {
                Ok(installed) => {
                    let args: &[&str] = if self.pin_on_finish { &["--pin"] } else { &[] };
                    if install::relaunch(&installed, args).is_ok() {
                        cx.quit();
                        return;
                    }
                }
                Err(err) => {
                    window.push_notification(Notification::error(format!("Couldn't install MulchLauncher: {err}")), cx);
                }
            }
        }
        if self.pin_on_finish {
            self.pin_pending = true;
        }
        cx.notify();
    }

    /// Asks Windows to pin the app; if Windows won't let it, says how to do it by hand.
    fn pin_to_taskbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pin_pending = false;
        if !install::request_taskbar_pin() {
            window.push_notification(
                Notification::info(
                    "Windows only lets you pin apps yourself: right-click MulchLauncher in the taskbar and choose \
                     \"Pin to taskbar\".",
                )
                .title("Pin to taskbar")
                .autohide(false),
                cx,
            );
        }
    }

    fn setup_panel(&self, step: SetupStep, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let platforms: Vec<&str> = {
            let mut names: Vec<&str> = self.games.iter().map(|g| g.platform.label()).collect();
            names.sort_unstable();
            names.dedup();
            names
        };
        let installing = !install::is_dev_build() && !install::is_installed_copy();

        let (title, body, actions): (&str, AnyElement, AnyElement) = match step {
            SetupStep::Welcome => {
                let summary = if self.scanning && self.games.is_empty() {
                    "Looking for your games…".to_string()
                } else if self.games.is_empty() {
                    "No installed games found yet. Install one with any launcher and it will show up here.".to_string()
                } else {
                    format!(
                        "Found {} game{} from {}. Nothing to set up: whenever MulchLauncher opens, it finds \
                         what's installed, wherever it's installed.",
                        self.games.len(),
                        if self.games.len() == 1 { "" } else { "s" },
                        platforms.join(", ")
                    )
                };
                (
                    "Welcome to MulchLauncher",
                    div().text_sm().text_color(muted).child(summary).into_any_element(),
                    Button::new("setup-next")
                        .primary()
                        .label("Next")
                        .on_click(cx.listener(|app, _, _, cx| app.set_setup_step(SetupStep::OtherGames, cx)))
                        .into_any_element(),
                )
            }
            SetupStep::OtherGames => (
                "Any other games?",
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(
                        "Games from anywhere else, like emulators, itch.io downloads or old installers, can be \
                         added by picking their .exe. You can always do this later with Add game.",
                    )
                    .into_any_element(),
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("setup-add")
                            .outline()
                            .icon(IconName::Plus)
                            .label("Add games…")
                            .on_click(cx.listener(|app, _, _, cx| app.add_games(cx))),
                    )
                    .child(
                        Button::new("setup-next")
                            .primary()
                            .label("Next")
                            .on_click(cx.listener(|app, _, _, cx| app.set_setup_step(SetupStep::Taskbar, cx))),
                    )
                    .into_any_element(),
            ),
            SetupStep::Taskbar => {
                let pinned_note = (!self.pinned_launchers.is_empty()).then(|| {
                    div()
                        .text_sm()
                        .text_color(muted)
                        .child(format!(
                            "{} {} pinned to your taskbar. MulchLauncher has a button for each, so you could \
                             unpin them: right-click each one in the taskbar and choose \"Unpin from taskbar\".",
                            self.pinned_launchers.join(", "),
                            if self.pinned_launchers.len() == 1 { "is" } else { "are" }
                        ))
                });
                (
                    "Taskbar",
                    v_flex()
                        .gap_3()
                        .child(
                            Checkbox::new("setup-pin")
                                .checked(self.pin_on_finish)
                                .label("Pin MulchLauncher to the taskbar")
                                .on_click(cx.listener(|app, checked: &bool, _, cx| {
                                    app.pin_on_finish = *checked;
                                    cx.notify();
                                })),
                        )
                        .children(pinned_note)
                        .into_any_element(),
                    Button::new("setup-finish")
                        .primary()
                        .label(if installing { "Install and finish" } else { "Finish" })
                        .on_click(cx.listener(|app, _, window, cx| app.finish_setup(window, cx)))
                        .into_any_element(),
                )
            }
        };

        let step_number = match step {
            SetupStep::Welcome => 1,
            SetupStep::OtherGames => 2,
            SetupStep::Taskbar => 3,
        };

        deferred(
            div()
                .id("setup-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.background.opacity(0.85))
                .child(
                    v_flex()
                        .w(px(440.))
                        .p_6()
                        .gap_4()
                        .rounded_lg()
                        .bg(theme.popover)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_lg()
                        .child(div().text_xs().text_color(muted).child(format!("Step {step_number} of 3")))
                        .child(div().text_xl().font_semibold().child(title))
                        .child(body)
                        .child(h_flex().justify_end().child(actions)),
                ),
        )
        .with_priority(3)
        .into_any_element()
    }

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let launcher_icons: Vec<_> =
            self.launchers.iter().enumerate().map(|(ix, launcher)| self.launcher_icon(ix, launcher, cx)).collect();
        let theme = cx.theme();

        h_flex()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .flex_shrink_0()
            .px_5()
            .gap_4()
            .border_b_1()
            .border_color(theme.border)
            .child(h_flex().flex_1().gap_3().children(launcher_icons))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("rescan")
                            .ghost()
                            .small()
                            .icon(IconName::RefreshCw)
                            .loading(self.scanning)
                            .tooltip("Scan again")
                            .on_click(cx.listener(|app, _, _, cx| app.rescan(cx))),
                    )
                    .child(
                        Button::new("add-game")
                            .primary()
                            .small()
                            .icon(IconName::Plus)
                            .label("Add game")
                            .on_click(cx.listener(|app, _, _, cx| app.add_games(cx))),
                    ),
            )
            .into_any_element()
    }

    fn launcher_icon(&self, ix: usize, launcher: &Launcher, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = launcher.name;
        let open = launcher.open.clone();

        div()
            .id(("launcher", ix))
            .size(px(LAUNCHER_SIZE))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(theme.list_hover))
            .child(launcher_glyph(name, launcher.icon.clone(), theme.muted_foreground))
            .tooltip(move |window, cx| Tooltip::new(format!("Open {name}")).build(window, cx))
            .on_click(move |_, window, cx| run_action(&open, name, window, cx))
            .into_any_element()
    }

    fn tile(&self, ix: usize, game: &Game, layout: &GridLayout, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let (width, height) = (layout.tile_width, layout.tile_width * COVER_ASPECT);

        let play_game = game.clone();
        let menu_game = game.clone();
        let app = cx.entity().downgrade();

        div()
            .id(("game", ix))
            .relative()
            .w(px(width))
            .cursor_pointer()
            .on_click(cx.listener(move |app, _, window, cx| app.request_play(play_game.clone(), window.mouse_position(), cx)))
            .child(
                div()
                    .w(px(width))
                    .h(px(height))
                    .overflow_hidden()
                    .bg(theme.muted)
                    .child(artwork(game, width)),
            )
            .child(div().pt_2().text_sm().font_medium().truncate().child(game.name.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).child(game.platform.label()))
            .context_menu(move |menu, _, _| game_menu(menu, &menu_game, app.clone()))
            .into_any_element()
    }
}

const LAUNCHER_SIZE: f32 = 36.;

/// How much of a tile's width an icon (rather than cover art) takes up.
const ICON_SHARE: f32 = 0.6;
const PLAY_CARD_WIDTH: f32 = 180.;
const PLAY_CARD_PADDING: f32 = 8.;
const PLAY_BUTTON_HEIGHT: f32 = 36.;
const GRID_PADDING: f32 = 20.;
const HEADER_HEIGHT: f32 = 72.;
/// Slack so pixel rounding never clips the last row.
const FIT_SLACK: f32 = 8.;
/// Width kept free on the right so a scrollbar never overlaps the last column.
const SCROLLBAR_ROOM: f32 = 8.;

/// Cover art fills as much of the tile as it can without cropping. Icons (already trimmed of transparent padding)
/// sit centred at a fixed share of the tile, so they all look the same size.
/// No words inside the tile: the name is always shown underneath.
fn artwork(game: &Game, width: f32) -> AnyElement {
    match &game.art {
        // Shown whole: scaled to fit inside the tile, never cropped or stretched.
        Some(Art::Cover(path)) => img(path.clone()).size_full().object_fit(ObjectFit::Contain).into_any_element(),
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

fn game_menu(menu: PopupMenu, game: &Game, app: WeakEntity<MulchApp>) -> PopupMenu {
    let play_game = game.clone();
    let play_app = app.clone();
    let mut menu = menu.item(PopupMenuItem::new("Play").on_click(move |_, window, cx| {
        let at = window.mouse_position();
        play_app.update(cx, |app, cx| app.request_play(play_game.clone(), at, cx)).ok();
    }));

    if let Some(show) = game.show_in_launcher.clone() {
        let name = game.name.clone();
        let launcher = match game.platform {
            Platform::Xbox => "Microsoft Store",
            Platform::Gog => "GOG Galaxy",
            other => other.label(),
        };
        menu = menu.item(
            PopupMenuItem::new(format!("Show in {launcher}")).on_click(move |_, window, cx| run_action(&show, &name, window, cx)),
        );
    }
    if let Some(dir) = game.install_dir.clone() {
        menu = menu.item(PopupMenuItem::new("Open folder").on_click(move |_, _, cx| cx.open_with_system(&dir)));
    }
    if game.uninstall.is_some() {
        let uninstall_game = game.clone();
        menu = menu.separator().item(
            PopupMenuItem::new(format!("Uninstall with {}…", game.platform.label()))
                .on_click(move |_, window, cx| confirm_uninstall(&uninstall_game, window, cx)),
        );
    }
    if game.platform == Platform::Manual {
        if let Action::Exe { path, .. } = &game.launch {
            let exe = path.clone();
            menu = menu.separator().item(PopupMenuItem::new("Remove from MulchLauncher").on_click(move |_, _, cx| {
                app.update(cx, |app, cx| app.remove_manual(exe.clone(), cx)).ok();
            }));
        }
    }
    menu
}

fn confirm_uninstall(game: &Game, window: &mut Window, cx: &mut App) {
    let game = game.clone();
    window.open_alert_dialog(cx, move |dialog, _, _| {
        let Some(uninstall) = game.uninstall.clone() else { return dialog };
        let name = game.name.clone();
        let platform = game.platform.label();
        dialog
            .title(SharedString::from(format!("Uninstall {}?", game.name)))
            .description(SharedString::from(format!(
                "Opens {platform}'s uninstaller for this game. {platform} will ask you to confirm."
            )))
            .confirm()
            .ok_text("Continue")
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                run_action(&uninstall, &name, window, cx);
                true
            })
    });
}

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let viewport = window.viewport_size();
        let layout = fit_tiles(
            self.games.len(),
            f32::from(viewport.width) - GRID_PADDING * 2. - SCROLLBAR_ROOM,
            f32::from(viewport.height) - HEADER_HEIGHT - GRID_PADDING * 2. - FIT_SLACK,
        );
        let header = self.header(cx);
        // Whole-pixel tile widths, and rows built explicitly rather than by
        // wrapping: with fractional widths, wrapping could push a row's last
        // tile down a line on some frames and back on others while resizing.
        let layout = GridLayout { tile_width: layout.tile_width.floor(), ..layout };
        let mut tiles: Vec<AnyElement> =
            self.games.iter().enumerate().map(|(ix, game)| self.tile(ix, game, &layout, cx)).collect();
        let mut rows = Vec::new();
        while !tiles.is_empty() {
            let rest = tiles.split_off(layout.columns.min(tiles.len()));
            rows.push(h_flex().items_start().gap(px(GRID_GAP)).children(std::mem::replace(&mut tiles, rest)));
        }
        let empty = !self.scanning && self.games.is_empty();

        let play_card = self.pending_play.as_ref().map(|pending| self.play_card(pending, cx));
        let setup_panel = self.setup.map(|step| self.setup_panel(step, cx));
        if self.pin_pending {
            self.pin_pending = false;
            cx.defer_in(window, |app, window, cx| app.pin_to_taskbar(window, cx));
        }

        v_flex()
            .relative()
            .size_full()
            .children(play_card)
            .children(setup_panel)
            .child(header)
            .child(
                div()
                    .id("library")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(GRID_PADDING))
                    // Tiles flow from the top left, across then down.
                    .flex()
                    .items_start()
                    .justify_start()
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
                                        .child("Install a game with any launcher, or use Add game to pick one yourself."),
                                ),
                        )
                    })
                    .child(v_flex().gap(px(GRID_GAP)).children(rows)),
            )
    }
}
