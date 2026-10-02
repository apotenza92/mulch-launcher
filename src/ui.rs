//! The main window: every detected game as a tile, plus buttons for the
//! launchers that are installed.

use crate::launch;
use crate::scan::{self, Action, Game, Launcher, Platform, ScanResult, manual};
use gpui_kit::component::button::*;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::theme::{ActiveTheme, Theme, ThemeMode};
use gpui_kit::component::{WindowExt, *};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;

const TILE_WIDTH: f32 = 168.;
const TILE_HEIGHT: f32 = 252.;

pub fn run() {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(|cx| {
        gpui_kit::init(cx);
        Theme::change(ThemeMode::Dark, None, cx);

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions { title: Some("Mulch".into()), ..Default::default() }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1240.), px(820.)), cx))),
            window_min_size: Some(size(px(560.), px(420.))),
            app_id: Some("MulchLauncher".into()),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(MulchApp::new)).expect("failed to open the Mulch window");
    });
}

struct MulchApp {
    games: Vec<Game>,
    launchers: Vec<Launcher>,
    scanning: bool,
    last_scan: Option<Duration>,
}

impl MulchApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut app = Self { games: Vec::new(), launchers: Vec::new(), scanning: false, last_scan: None };
        app.rescan(cx);
        app
    }

    /// Scans on a background thread so the window opens instantly.
    fn rescan(&mut self, cx: &mut Context<Self>) {
        if self.scanning {
            return;
        }
        self.scanning = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_spawn(async move { scan::scan_all() }).await;
            this.update(cx, |app, cx| app.apply(result, cx)).ok();
        })
        .detach();
    }

    fn apply(&mut self, result: ScanResult, cx: &mut Context<Self>) {
        self.games = result.games;
        self.launchers = result.launchers;
        self.last_scan = Some(result.total);
        self.scanning = false;
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

    fn header(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let status = if self.scanning && self.games.is_empty() {
            "Looking for your games…".to_string()
        } else {
            let count = self.games.len();
            let timing = self.last_scan.map(|t| format!(" · found in {} ms", t.as_millis())).unwrap_or_default();
            format!("{count} game{}{timing}", if count == 1 { "" } else { "s" })
        };

        let launcher_buttons = self.launchers.iter().enumerate().map(|(ix, launcher)| {
            let open = launcher.open.clone();
            let name = launcher.name;
            Button::new(("launcher", ix))
                .ghost()
                .small()
                .label(name)
                .tooltip(format!("Open {name}"))
                .on_click(move |_, window, cx| run_action(&open, name, window, cx))
        });

        h_flex()
            .w_full()
            .px_5()
            .py_3()
            .gap_4()
            .border_b_1()
            .border_color(theme.border)
            .child(
                v_flex()
                    .child(div().text_xl().font_semibold().child("Mulch"))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(status)),
            )
            .child(h_flex().flex_1().gap_1().flex_wrap().children(launcher_buttons))
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
            )
            .into_any_element()
    }

    fn tile(&self, ix: usize, game: &Game, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let cover = match &game.art {
            Some(path) => img(path.clone()).size_full().object_fit(ObjectFit::Cover).into_any_element(),
            None => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .p_3()
                .text_center()
                .text_lg()
                .font_semibold()
                .text_color(theme.muted_foreground)
                .child(game.name.clone())
                .into_any_element(),
        };

        let launch = game.launch.clone();
        let name = game.name.clone();
        let menu_game = game.clone();
        let app = cx.entity().downgrade();

        div()
            .id(("game", ix))
            .w(px(TILE_WIDTH))
            .cursor_pointer()
            .on_click(move |_, window, cx| run_action(&launch, &name, window, cx))
            .child(
                div()
                    .w(px(TILE_WIDTH))
                    .h(px(TILE_HEIGHT))
                    .rounded_lg()
                    .overflow_hidden()
                    .bg(theme.muted)
                    .border_1()
                    .border_color(theme.border)
                    .hover(|style| style.border_color(theme.primary))
                    .child(cover),
            )
            .child(div().pt_2().text_sm().font_medium().truncate().child(game.name.clone()))
            .child(div().text_xs().text_color(theme.muted_foreground).child(game.platform.label()))
            .context_menu(move |menu, _, _| game_menu(menu, &menu_game, app.clone()))
            .into_any_element()
    }
}

fn game_menu(menu: PopupMenu, game: &Game, app: WeakEntity<MulchApp>) -> PopupMenu {
    let launch = game.launch.clone();
    let name = game.name.clone();
    let mut menu = menu.item(PopupMenuItem::new("Play").on_click(move |_, window, cx| run_action(&launch, &name, window, cx)));

    if let Some(dir) = game.install_dir.clone() {
        menu = menu.item(PopupMenuItem::new("Open folder").on_click(move |_, _, cx| cx.reveal_path(&dir)));
    }
    if let Some(uninstall) = game.uninstall.clone() {
        let name = game.name.clone();
        menu = menu.separator().item(
            PopupMenuItem::new(format!("Uninstall with {}", game.platform.label()))
                .on_click(move |_, window, cx| run_action(&uninstall, &name, window, cx)),
        );
    }
    if game.platform == Platform::Manual {
        if let Action::Exe { path, .. } = &game.launch {
            let exe = path.clone();
            menu = menu.separator().item(PopupMenuItem::new("Remove from Mulch").on_click(move |_, _, cx| {
                app.update(cx, |app, cx| app.remove_manual(exe.clone(), cx)).ok();
            }));
        }
    }
    menu
}

fn run_action(action: &Action, name: &str, window: &mut Window, cx: &mut App) {
    if let Err(err) = launch::run(action) {
        window.push_notification(Notification::error(format!("Couldn't start {name}: {err}")), cx);
    }
}

impl Render for MulchApp {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = self.header(cx);
        let tiles: Vec<_> = self.games.iter().enumerate().map(|(ix, game)| self.tile(ix, game, cx)).collect();
        let empty = !self.scanning && self.games.is_empty();

        v_flex()
            .size_full()
            .child(header)
            .child(
                div()
                    .id("library")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_5()
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
                    .child(div().flex().flex_wrap().gap_5().children(tiles)),
            )
    }
}
