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
    COVER_ASPECT, DEFAULT_SIZE, GRID_GAP, GridLayout, HEADING_HEIGHT, LABEL_HEIGHT, TILE_SIZES, grid_width,
    layout as grid_layout,
};
use mulch_launcher::scan::{self, Action, Art, Game, Launcher, Platform, ScanResult};
use mulch_launcher::settings::Settings;
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
        Theme::change(ThemeMode::Dark, None, cx);

        let options = WindowOptions {
            // Our own slim title bar (see `title_bar`), with the toolbar under
            // it. The title is still set for the taskbar and Alt+Tab.
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                default_window_size(saved_tile_width(), cx),
                cx,
            ))),
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
        Theme::change(ThemeMode::Dark, None, cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some(APP_NAME.into()), ..TitleBar::title_bar_options() }),
            app_owns_titlebar_drag: true,
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(440.), px(250.)), cx))),
            is_resizable: false,
            app_id: Some(APP_NAME.into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Installer { desktop: true, error: None }))
            .expect("failed to open the window");
    });
}

struct Installer {
    desktop: bool,
    error: Option<String>,
}

impl Installer {
    fn install(&mut self, cx: &mut Context<Self>) {
        let result = std::env::current_exe().and_then(|download| {
            let installed = install::install(self.desktop)?;
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
        v_flex()
            .size_full()
            .bg(theme.background)
            .child(TitleBar::new().bg(theme.background).border_color(theme.background))
            .child(
                v_flex()
                    .flex_1()
                    .px_6()
                    .pb_6()
                    .gap_4()
                    .child(div().text_xl().font_semibold().child(format!("Install {APP_NAME}")))
                    .child(
                        div()
                            .text_sm()
                            .text_color(muted)
                            .child("Every installed game, from every launcher. Installs just for you; it'll be in your Start menu."),
                    )
                    .child(
                        Checkbox::new("desktop")
                            .checked(self.desktop)
                            .label("Add to desktop")
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.desktop = *checked;
                                cx.notify();
                            })),
                    )
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
    settings: Settings,
    /// "Add game manually", while it's showing.
    add_panel: Option<AddPanel>,
    /// Natural widths of the toolbar's left and right groups, measured as
    /// they're laid out, which set the window's minimum width.
    toolbar_widths: Rc<[Cell<f32>; 2]>,
    /// The native window, once known.
    hwnd: Option<isize>,
    /// The game tile under the mouse, which shows its full name.
    hovered_tile: Option<usize>,
    /// The tile width the window's width last snapped to.
    snapped_tile: Option<f32>,
    scanning: bool,
    /// A game the user clicked, waiting for a second click on Play.
    pending_play: Option<PendingPlay>,
    /// When each game was last played, for sorting most recent first.
    history: History,
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

struct PendingPlay {
    game: Game,
    /// Where the click happened; the Play button opens under it.
    at: Point<Pixels>,
}

impl MulchApp {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let settings = Settings::load();
        let mut app = Self {
            games: Vec::new(),
            launchers: Vec::new(),
            social: Vec::new(),
            settings,
            add_panel: None,
            toolbar_widths: Rc::default(),
            hwnd: None,
            hovered_tile: None,
            snapped_tile: None,
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
        for game in &mut self.games {
            if !matches!(game.art, Some(Art::Cover(_))) {
                if let Some(art) = shown.remove(&game.id) {
                    game.art = Some(art);
                }
            }
        }
        self.history.sort(&mut self.games);
        self.launchers = result.launchers;
        self.launchers.sort_by_key(|l| l.name.to_lowercase());
        self.social = result.social;
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
            .children(self.secondary_action(&pending.game, cx))
            .children(pending.game.install_dir.clone().map(|dir| {
                card_button("show-folder", IconName::FolderOpen, "Show in folder").on_click(cx.listener(
                    move |app, _, _, cx| {
                        cx.open_with_system(&dir);
                        app.cancel_play(cx);
                    },
                ))
            }));

        deferred(
            div()
                .id("play-backdrop")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                // Not blocking: a click elsewhere closes the card and still
                // reaches what's under it, so clicking another game opens its
                // card straight away. The card itself blocks clicks.
                .on_mouse_down(MouseButton::Left, cx.listener(|app, _, _, cx| app.cancel_play(cx)))
                .on_mouse_down(MouseButton::Right, cx.listener(|app, _, _, cx| app.cancel_play(cx)))
                .child(
                    anchored()
                        .position_mode(AnchoredPositionMode::Window)
                        .position(pending.at)
                        // Put the middle of the Play button under the cursor.
                        .offset(point(px(-PLAY_CARD_WIDTH / 2.), px(-(PLAY_CARD_PADDING + PLAY_BUTTON_HEIGHT / 2.))))
                        .snap_to_window_with_margin(px(8.))
                        .child(card),
                ),
        )
        .with_priority(2)
        .into_any_element()
    }

    /// "Show in <launcher>", or for games the user added (which have no
    /// launcher) the only way to take them out again.
    fn secondary_action(&self, game: &Game, cx: &mut Context<Self>) -> Option<Button> {
        if let Some(show) = game.show_in_launcher.clone() {
            let launcher = match game.platform {
                Platform::Xbox => "Microsoft Store",
                Platform::Gog => "GOG Galaxy",
                other => other.label(),
            };
            let name = game.name.clone();
            return Some(card_button("show-launcher", IconName::ExternalLink, format!("Show in {launcher}")).on_click(
                cx.listener(move |app, _, window, cx| {
                    run_action(&show, &name, window, cx);
                    app.cancel_play(cx);
                }),
            ));
        }
        if let (Platform::Manual, Action::Exe { path, .. }) = (game.platform, &game.launch) {
            let exe = path.clone();
            return Some(
                card_button("remove-manual", IconName::Close, "Remove")
                    .on_click(cx.listener(move |app, _, _, cx| app.remove_manual(exe.clone(), cx))),
            );
        }
        None
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
        cx.notify();
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

    fn remove_manual(&mut self, exe: PathBuf, cx: &mut Context<Self>) {
        let _ = manual::remove(&exe);
        self.rescan(cx);
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        TitleBar::new()
            .bg(theme.background)
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

    /// One centred row of groups, split by dividers: chat apps | launchers |
    /// zoom out, default size, zoom in | scan again, add game.
    fn toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let launcher_icons: Vec<AnyElement> = self
            .launchers
            .iter()
            .enumerate()
            .map(|(ix, launcher)| self.launcher_icon(("launcher", ix), launcher, cx))
            .collect();
        let social_icons: Vec<AnyElement> =
            self.social.iter().enumerate().map(|(ix, app)| self.launcher_icon(("social", ix), app, cx)).collect();
        let zoom = vec![
            quick_tooltip(
                "zoom-out-tip",
                "Smaller",
                Button::new("zoom-out")
                    .ghost()
                    .icon(Icon::empty().path("mulch/zoom-out.svg"))
                    .disabled(self.tile_size() == 0)
                    .on_click(cx.listener(|app, _, _, cx| app.zoom(-1, cx))),
            )
            .into_any_element(),
            quick_tooltip(
                "zoom-reset-tip",
                "Default size",
                Button::new("zoom-reset")
                    .ghost()
                    .icon(Icon::empty().path("mulch/search.svg"))
                    .disabled(self.tile_size() == DEFAULT_SIZE)
                    .on_click(cx.listener(|app, _, _, cx| app.reset_zoom(cx))),
            )
            .into_any_element(),
            quick_tooltip(
                "zoom-in-tip",
                "Bigger",
                Button::new("zoom-in")
                    .ghost()
                    .icon(Icon::empty().path("mulch/zoom-in.svg"))
                    .disabled(self.tile_size() == TILE_SIZES.len() - 1)
                    .on_click(cx.listener(|app, _, _, cx| app.zoom(1, cx))),
            )
            .into_any_element(),
        ];
        let actions = vec![
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

        let border = cx.theme().border;
        let mut row = h_flex().gap(px(TOOLBAR_GAP));
        let groups = [social_icons, launcher_icons, zoom, actions].into_iter().filter(|group| !group.is_empty());
        for (ix, group) in groups.enumerate() {
            if ix > 0 {
                row = row.child(div().w(px(1.)).h(px(LAUNCHER_SIZE - 10.)).bg(border));
            }
            row = row.child(h_flex().gap_2().children(group));
        }

        h_flex()
            .flex_shrink_0()
            .h(px(TOOLBAR_HEIGHT))
            .px(px(TOOLBAR_PADDING))
            .justify_center()
            .child(self.measured(0, row))
            .into_any_element()
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

    /// The chosen tile size (see `layout::TILE_SIZES`).
    fn tile_size(&self) -> usize {
        self.settings.tile_size.unwrap_or(DEFAULT_SIZE).min(TILE_SIZES.len() - 1)
    }

    /// Steps the tile size up or down one preset, and remembers it.
    fn zoom(&mut self, step: isize, cx: &mut Context<Self>) {
        let size = self.tile_size().saturating_add_signed(step).min(TILE_SIZES.len() - 1);
        self.settings.tile_size = Some(size);
        self.settings.save();
        cx.notify();
    }

    fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.settings.tile_size = None;
        self.settings.save();
        cx.notify();
    }

    /// The narrowest the window can be with the whole toolbar still showing.
    fn toolbar_min_width(&self) -> f32 {
        TOOLBAR_PADDING * 2. + self.toolbar_widths[0].get()
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
            let tile = TILE_SIZES[self.tile_size()];
            // Snap to the new tile size straight away after zooming.
            let zoomed = self.snapped_tile.replace(tile).is_some_and(|before| before != tile);
            crate::window_size::set_snap(hwnd, GRID_MARGIN_X * 2., tile + GRID_GAP, GRID_GAP, zoomed);
            crate::window_size::set_min(hwnd, self.toolbar_min_width().max(MIN_WINDOW_WIDTH));
        }
    }

    /// "Add game manually": games found on this PC, ticked, plus a file picker
    /// for anything it missed.
    fn add_panel(&self, panel: &AddPanel, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let body = match &panel.suggestions {
            None => div().text_sm().text_color(muted).child("Looking for games on this PC…").into_any_element(),
            Some(found) if found.is_empty() => div()
                .text_sm()
                .text_color(muted)
                .child("No other games found. Use Browse to pick a game's .exe yourself.")
                .into_any_element(),
            Some(found) => v_flex()
                .id("suggestions")
                .max_h(px(320.))
                .overflow_y_scroll()
                .gap_3()
                .children(found.iter().enumerate().map(|(ix, suggestion)| {
                    let exe = suggestion.exe.clone();
                    h_flex()
                        .gap_3()
                        .items_start()
                        .child(
                            Checkbox::new(("suggestion", ix))
                                .checked(panel.selected.contains(&suggestion.exe))
                                .on_click(
                                    cx.listener(move |app, _: &bool, _, cx| app.toggle_suggestion(exe.clone(), cx)),
                                ),
                        )
                        .child(
                            v_flex()
                                .min_w_0()
                                .child(div().text_sm().font_medium().child(suggestion.name.clone()))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .truncate()
                                        .child(suggestion.exe.display().to_string()),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted)
                                        .child(format!("Looks like a game: {}", suggestion.reason)),
                                ),
                        )
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
                .bg(theme.background.opacity(0.85))
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
        let theme = cx.theme();
        let (width, height) = (layout.tile_width, layout.tile_width * COVER_ASPECT);

        let play_game = game.clone();

        // The full name shows on hover only when it's cut short.
        let show_full_name = self.hovered_tile == Some(ix) && name_is_cut_short(&game.name, width, window, cx);
        div()
            .id(("game", ix))
            .relative()
            .w(px(width))
            .on_hover(cx.listener(move |app, hovered: &bool, _, cx| {
                if *hovered {
                    app.hovered_tile = Some(ix);
                } else if app.hovered_tile == Some(ix) {
                    app.hovered_tile = None;
                }
                cx.notify();
            }))
            .on_click(
                cx.listener(move |app, _, window, cx| app.request_play(play_game.clone(), window.mouse_position(), cx)),
            )
            .child(div().w(px(width)).h(px(height)).overflow_hidden().bg(theme.muted).child(artwork(game, width)))
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
                                    .bg(theme.popover)
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
            .into_any_element()
    }
}

const LAUNCHER_SIZE: f32 = 36.;
/// A game name's line under its tile (text_sm).
const NAME_LINE_HEIGHT: f32 = 20.;
/// Padding around a hovered game's full name (horizontal, vertical; the border adds 1).
const NAME_PILL_PADDING: (f32, f32) = (9., 3.);
/// Labels for the recency groups (see `recency_groups`).
const GROUP_NAMES: [&str; 3] = ["Played in the last week", "Played in the last month", "Everything else"];

/// How much of a tile's width an icon (rather than cover art) takes up.
const ICON_SHARE: f32 = 0.6;
const PLAY_CARD_WIDTH: f32 = 180.;
const PLAY_CARD_PADDING: f32 = 8.;
const PLAY_BUTTON_HEIGHT: f32 = 36.;
/// Space above and below the grid.
const GRID_PADDING: f32 = 20.;
/// Least space either side of the grid (the window snaps to widths where it's exactly this).
const GRID_MARGIN_X: f32 = 24.;
/// The title bar's height.
const TITLE_BAR_HEIGHT: f32 = 34.;
/// The toolbar under the title bar.
const TOOLBAR_HEIGHT: f32 = 52.;
const TOOLBAR_PADDING: f32 = 16.;
const TOOLBAR_GAP: f32 = 16.;
/// The window is never narrower than this, even with a short toolbar.
const MIN_WINDOW_WIDTH: f32 = 480.;
/// Minimise, maximise and close, on the right of the title bar, less its left padding.
const WINDOW_CONTROLS_WIDTH: f32 = 3. * 34. - 12.;

/// The tile width the user last chose (the default size if never).
fn saved_tile_width() -> f32 {
    TILE_SIZES[Settings::load().tile_size.unwrap_or(DEFAULT_SIZE).min(TILE_SIZES.len() - 1)]
}

/// Opening size, at the chosen tile size: exactly 6 games across, and tall
/// enough for the three labelled groups (last week, last month, everything
/// else) with one row each, the last cut off halfway so it's clear there's
/// more below. Smaller if the screen is.
fn default_window_size(tile: f32, cx: &App) -> gpui_kit::Size<Pixels> {
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
    let font = Font { weight: FontWeight::MEDIUM, ..font(cx.theme().font_family.clone()) };
    let run = TextRun { len: name.len(), font, color: Hsla::default(), background_color: None, underline: None, strikethrough: None };
    let font_size = rems(0.875).to_pixels(window.rem_size());
    let line = window.text_system().shape_line(SharedString::from(name.to_string()), font_size, &[run], None);
    f32::from(line.width) > width
}

/// A full-width, left-aligned button for the play card.
fn card_button(id: &'static str, icon: IconName, label: impl Into<SharedString>) -> Button {
    Button::new(id).ghost().small().w_full().justify_start().icon(icon).label(label)
}

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_min_width(window);
        let viewport = window.viewport_size();
        let layout =
            grid_layout(&self.recency_groups(), f32::from(viewport.width) - GRID_MARGIN_X * 2., self.tile_size());
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
            // Everything centred: labels, and every row (a short last row too).
            sections.push(
                v_flex()
                    .items_center()
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

        let play_card = self.pending_play.as_ref().map(|pending| self.play_card(pending, cx));
        let add_panel = self.add_panel.as_ref().map(|panel| self.add_panel(panel, cx));

        v_flex().relative().size_full().children(play_card).children(add_panel).child(title_bar).child(toolbar).child(
            div()
                .id("library")
                .flex_1()
                .overflow_y_scroll()
                .py(px(GRID_PADDING))
                .px(px(GRID_MARGIN_X))
                // Played in the last week, then the last month, then the rest,
                // all centred (exactly, given the window's snapped widths).
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
