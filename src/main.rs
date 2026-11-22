#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(test)]
mod app_tests;
mod app_ui;
mod card;
mod command;
mod desktop;
mod document;
mod geometry;
mod interface;
mod network;
#[cfg(test)]
mod property_tests;
mod scene;
mod sharing;
mod storage;
mod story_map;
mod templates;
mod terminal;
mod workspace;

use anyhow::{ensure, Context, Result};
use eframe::egui::{self, Color32};
use interface::Preview;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use workspace::Workspace;

#[derive(Clone)]
enum Dialog {
    Settings(String),
    Delete(String),
    Join,
    Templates,
    Properties,
}

struct Canvas {
    workspace: Workspace,
    scene: scene::Scene,
    status: String,
    last_save: Instant,
    dialog: Option<Dialog>,
    previews: HashMap<String, Preview>,
    template_previews: Vec<Preview>,
    network: network::Network,
    network_status: String,
    join_link: String,
    net_dirty: bool,
    joining: bool,
    last_publish: Instant,
    start_link: bool,
    instance: desktop::Instance,
    receive_retry: bool,
    network_error: Option<String>,
    display_name: String,
    retry_time: Instant,
    home: bool,
    copied_for: Option<String>,
    copied_at: Instant,
    preferences: std::path::PathBuf,
    terminal: terminal::Terminal,
    invitation: Option<String>,
    copy_invitation: bool,
    graphical: bool,
    quit: bool,
}
impl Canvas {
    fn new(
        root: std::path::PathBuf,
        instance: desktop::Instance,
        ctx: Option<&egui::Context>,
        link: Option<String>,
        live: bool,
    ) -> Result<Self> {
        let preferences = root.join("editor-name.txt");
        let graphical = ctx.is_some();
        let template_previews = if let Some(ctx) = ctx {
            interface::theme(ctx);
            (0..templates::NAMES.len())
                .map(|index| {
                    let mut board = document::Board::new()?;
                    templates::insert(&mut board, index)?;
                    Ok(Preview::new(&board))
                })
                .collect::<Result<_>>()?
        } else {
            Vec::new()
        };
        Ok(Self {
            workspace: Workspace::load(root.clone())?,
            scene: scene::Scene::default(),
            status: "Saved locally".into(),
            last_save: Instant::now(),
            dialog: None,
            previews: HashMap::new(),
            template_previews,
            network: network::Network::start(live.then_some(root)),
            network_status: "Sharing is off".into(),
            join_link: link.unwrap_or_default(),
            net_dirty: false,
            joining: false,
            last_publish: Instant::now(),
            start_link: true,
            instance,
            receive_retry: false,
            network_error: None,
            display_name: std::fs::read_to_string(&preferences)
                .unwrap_or_else(|_| "Editor".into())
                .chars()
                .take(64)
                .collect(),
            home: true,
            copied_for: None,
            copied_at: Instant::now(),
            preferences,
            retry_time: Instant::now(),
            terminal: terminal::Terminal::default(),
            invitation: None,
            copy_invitation: false,
            graphical,
            quit: false,
        })
    }
    fn reset_view(&mut self) {
        self.scene.reset();
    }
    fn canvas_result(&mut self, result: anyhow::Result<bool>) {
        match result {
            Ok(true) => self.result(Ok(())),
            Ok(false) => {}
            Err(error) => {
                self.workspace.dirty = self.workspace.board().is_some();
                self.result(Err(error));
            }
        }
    }
    fn flush(&mut self) -> Result<()> {
        self.workspace.flush()?;
        self.status = "Saved locally".into();
        self.last_save = Instant::now();
        Ok(())
    }
    fn save(&mut self) {
        if let Err(error) = self.flush() {
            self.status = format!("Save failed: {error:#}");
        }
    }
    fn save_before_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested()) {
            if let Err(error) = self.flush() {
                self.status = format!("Could not close because saving failed: {error:#}");
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
    }
    fn record_gui<T>(&mut self, line: String, result: Result<T>) {
        match result {
            Ok(_) => self.terminal.record(line),
            Err(error) => self.status = format!("Error: {error:#}"),
        }
    }
    fn run_gui(&mut self, args: &[&str]) {
        let result = self.execute(&args.iter().map(|s| (*s).into()).collect::<Vec<_>>(), true);
        self.record_gui(command::format(args), result);
    }
    fn result(&mut self, result: anyhow::Result<()>) {
        match result {
            Ok(()) => {
                if let Some(board) = self.workspace.board() {
                    self.previews.remove(&board.id());
                }
                self.workspace.dirty = true;
                self.net_dirty = true;
                self.status = "Unsaved changes".into();
            }
            Err(e) => self.status = format!("Error: {e:#}"),
        }
    }
    fn board_index(&self, id: &str) -> Option<usize> {
        self.workspace
            .entries
            .iter()
            .position(|e| e.board.id() == id)
    }

    fn create_board(&mut self, template: Option<usize>) -> Result<()> {
        self.workspace.create_board(template)?;
        self.reset_view();
        self.home = false;
        if template.is_some() {
            self.fit_view()?;
            self.scene.fit_on_show = self.graphical;
        }
        self.status = "Saved locally".into();
        Ok(())
    }

    fn rename_board(&mut self, index: usize, title: &str) -> Result<()> {
        if self.workspace.active == Some(index) {
            self.workspace.entries[index].board.rename(title)?;
            self.result(Ok(()));
        } else {
            self.workspace.rename(index, title)?;
        }
        Ok(())
    }

    fn close_board(&mut self, index: usize) -> Result<()> {
        self.workspace.close(index)?;
        self.reset_view();
        self.home |= self.workspace.active.is_none();
        self.status = "Saved locally".into();
        Ok(())
    }

    fn delete_board(&mut self, index: usize) -> Result<std::path::PathBuf> {
        let id = self.workspace.entries[index].board.id();
        let path = self.workspace.delete(index)?;
        self.previews.remove(&id);
        self.reset_view();
        self.home |= self.workspace.active.is_none();
        self.status = format!("Board moved to {}", path.display());
        Ok(path)
    }

    fn set_editor_name(&mut self, name: &str) -> Result<()> {
        ensure!(
            name.chars().count() <= 64,
            "Editor name is limited to 64 characters"
        );
        storage::write_atomic(&self.preferences, name.as_bytes())?;
        self.display_name = name.into();
        Ok(())
    }

    fn go_home(&mut self) {
        self.home = true;
        self.scene.pointer = None;
        self.scene.cancel_edit();
    }

    fn release_keyboard(ctx: &egui::Context) {
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }

    fn cancel_interaction(&mut self) {
        self.scene.cancel_edit();
        self.dialog = None;
    }

    fn fit_view(&mut self) -> Result<()> {
        let board = self
            .workspace
            .board()
            .ok_or_else(|| anyhow::anyhow!("No active board"))?;
        self.scene.fit(board);
        Ok(())
    }

    fn join_board(&mut self, link: &str) -> Result<()> {
        ensure!(!self.joining, "A join is already in progress");
        let ticket = sharing::Invitation::parse(link, sharing::now())?;
        self.workspace.flush()?;
        let local = self
            .workspace
            .entries
            .iter_mut()
            .find(|e| e.board.id() == ticket.body.board)
            .map(|e| e.board.save());
        self.network.join(ticket, local)?;
        self.invitation = None;
        self.copied_for = None;
        self.copy_invitation = false;
        self.joining = true;
        self.network_error = None;
        self.network_status = "Waiting for an online participant…".into();
        Ok(())
    }

    fn join_gui(&mut self) {
        let link = self.join_link.clone();
        match self.join_board(&link) {
            Ok(()) => self.terminal.record("share join <invitation>".into()),
            Err(error) => self.network_status = format!("{error:#}"),
        }
    }
    fn poll_network(&mut self, ctx: Option<&egui::Context>) {
        while let Ok(event) = self.network.events.try_recv() {
            match event {
                network::Event::Presence(epoch, peers) if epoch == self.network.epoch => {
                    self.scene.remote = peers
                        .into_iter()
                        .map(|(id, p)| {
                            let hash = id
                                .bytes()
                                .fold(0_u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32));
                            scene::RemotePresence {
                                name: p.name,
                                cursor: p.cursor,
                                selected: p.selected,
                                color: Color32::from_rgb(
                                    80 + (hash % 130) as u8,
                                    60 + ((hash / 131) % 130) as u8,
                                    90 + ((hash / 17033) % 130) as u8,
                                ),
                            }
                        })
                        .collect();
                }
                network::Event::Fatal(text) => {
                    self.network_error = Some(text);
                    self.network.board = None;
                    self.invitation = None;
                    self.copy_invitation = false;
                    self.network.epoch += 1;
                    self.joining = false;
                }
                network::Event::Failed(epoch, text, terminal)
                    if epoch == 0 || epoch == self.network.epoch =>
                {
                    self.network_error = Some(text);
                    if terminal {
                        let _ = self.network.stop();
                        self.invitation = None;
                        self.copy_invitation = false;
                        self.network_status = "Sharing is off".into();
                        self.scene.remote.clear();
                        self.joining = false;
                    }
                }
                network::Event::Status(epoch, text)
                    if epoch == 0 || epoch == self.network.epoch =>
                {
                    self.network_status = text
                }
                network::Event::Invitation(epoch, link) if epoch == self.network.epoch => {
                    if let Some(ctx) = ctx.filter(|_| self.copy_invitation) {
                        ctx.output_mut(|o| o.copied_text = link.clone());
                        self.copied_for = self.network.board.clone();
                        self.copied_at = Instant::now();
                    }
                    self.copy_invitation = false;
                    self.invitation = Some(link);
                    if self.network_status == "Preparing invitation…" {
                        self.network_status = "Invitation ready".into();
                    }
                }
                network::Event::Document(epoch, bytes) if epoch == self.network.epoch => {
                    let accepts = self.joining
                        || self.workspace.board().map(document::Board::id) == self.network.board;
                    if accepts {
                        match self.workspace.receive(&bytes) {
                            Ok(index) => {
                                self.previews
                                    .remove(&self.workspace.entries[index].board.id());
                                if self.joining {
                                    self.home = false;
                                    self.dialog = None;
                                    self.scene.reset();
                                    self.scene.fit(&self.workspace.entries[index].board);
                                    self.scene.fit_on_show = self.graphical;
                                }
                                self.joining = false;
                                self.receive_retry = false;
                                self.status = "Saved locally".into();
                                self.net_dirty = true;
                            }
                            Err(e) => {
                                self.receive_retry = true;
                                self.status = format!("Received board could not be saved: {e}");
                                self.network_status = self.status.clone();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    fn template(&mut self, index: usize) -> Result<String> {
        let board = self
            .workspace
            .board_mut()
            .ok_or_else(|| anyhow::anyhow!("No active board"))?;
        let previous: std::collections::HashSet<_> =
            board.cards().into_iter().map(|c| c.id).collect();
        let id = templates::insert(board, index)?;
        let added: Vec<_> = board
            .cards()
            .into_iter()
            .filter(|c| !previous.contains(&c.id))
            .collect();
        self.scene.select(Some(id.clone()));
        self.scene.fit_cards(&added);
        self.result(Ok(()));
        Ok(id)
    }
}

impl Canvas {
    fn open_board(&mut self, index: usize) -> Result<()> {
        if self.workspace.active != Some(index) {
            self.workspace.open(index)?;
            self.reset_view();
            self.status = "Saved locally".into();
        }
        self.home = false;
        Ok(())
    }

    fn share(&mut self, index: usize, copy: bool) -> Result<()> {
        let id = self.workspace.entries[index].board.id();
        if self.workspace.active != Some(index) {
            self.workspace.open(index)?;
            self.reset_view();
        }
        self.network
            .share(id, self.workspace.entries[index].board.save())?;
        self.copied_for = None;
        self.invitation = None;
        self.copy_invitation = copy;
        self.joining = false;
        self.network_error = None;
        self.network_status = "Preparing invitation…".into();
        Ok(())
    }

    fn stop_sharing(&mut self) -> Result<()> {
        if self.network.board.is_some() {
            self.network.stop()?;
        }
        self.joining = false;
        self.copied_for = None;
        self.invitation = None;
        self.copy_invitation = false;
        self.network_status = "Sharing is off".into();
        self.scene.remote.clear();
        Ok(())
    }

    fn command_reply(&mut self, args: &[String], persistent: bool) -> desktop::Reply {
        let result = self.execute(args, persistent).and_then(|value| {
            if self.workspace.dirty && !command::is_query(args) {
                self.flush()
                    .context("Command applied in memory but local save failed")?;
            }
            self.update_share();
            Ok(value)
        });
        result.into()
    }

    fn poll(&mut self, ctx: Option<&egui::Context>) {
        while let Ok(link) = self.instance.links.try_recv() {
            self.join_link = link;
            self.start_link = true;
        }
        self.poll_network(ctx);
        for _ in 0..8 {
            if self.quit {
                break;
            }
            let Ok(request) = self.instance.commands.try_recv() else {
                break;
            };
            let reply = self.command_reply(&request.args, true);
            if reply.ok() && !command::is_query(&request.args) {
                let args: Vec<_> = request.args.iter().map(String::as_str).collect();
                self.terminal.record(command::format(&args));
            }
            request.respond(&reply);
        }
        if self.copied_at.elapsed() > Duration::from_secs(4) {
            self.copied_for = None;
        }
        if self.network.board.is_none() {
            self.scene.remote.clear();
        }
        if self.start_link {
            self.start_link = false;
            if !self.join_link.is_empty() {
                if self.graphical {
                    self.dialog = Some(Dialog::Join);
                }
                self.join_gui();
            }
        }
    }

    fn update_share(&mut self) {
        if self.network.board.is_some()
            && !self.joining
            && self.workspace.board().map(document::Board::id) != self.network.board
            && self.stop_sharing().is_ok()
        {
            self.network_status = "Sharing stopped because the board was closed or changed".into();
        }
    }

    fn tick(&mut self) {
        self.update_share();
        if self.net_dirty
            && self.network.board.is_some()
            && !self.joining
            && self.last_publish.elapsed() > Duration::from_millis(200)
        {
            if let Some(board) = self.workspace.board_mut() {
                match self.network.publish(board.save()) {
                    Ok(()) => {
                        self.net_dirty = false;
                        self.network_error = None;
                        self.last_publish = Instant::now();
                    }
                    Err(e) => self.network_status = e.to_string(),
                }
            }
        }
        if self.receive_retry
            && self.network.board.is_some()
            && self.retry_time.elapsed() > Duration::from_secs(2)
        {
            let _ = self.network.refresh();
            self.retry_time = Instant::now();
        }
        self.network.presence(network::Presence {
            name: self.display_name.clone(),
            cursor: self
                .scene
                .pointer
                .filter(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.)),
            selected: self.scene.selected.clone(),
        });
        if self.workspace.dirty && self.last_save.elapsed() >= Duration::from_millis(500) {
            self.save();
        }
    }
}
fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    attach_parent_console(&args);
    match args.first().map(String::as_str) {
        Some("cli") => {
            let reply = cli(&args[1..]).unwrap_or_else(|error| Err(error).into());
            println!(
                "{}",
                serde_json::to_string(&reply).expect("Command replies are serializable")
            );
            if !reply.ok() {
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("--headless") => {
            if let Err(error) = headless(&args[1..]) {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("jotflock [INVITATION]     Open the desktop application\njotflock --headless       Run without a window\njotflock cli help         List commands\njotflock --install-desktop");
            return Ok(());
        }
        _ => {}
    }
    let setup = (|| -> Result<_> {
        if args.iter().any(|s| s == "--install-desktop") {
            command::desktop_install()?;
            return Ok(None);
        }
        let link = args.into_iter().find(|s| s.starts_with(sharing::PREFIX));
        let root = storage::root()?;
        Ok(desktop::instance(&root, link.as_deref())?.map(|instance| (root, instance, link)))
    })();
    let (root, instance, link) = match setup {
        Ok(Some(setup)) => setup,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("{error:#}");
            std::process::exit(1);
        }
    };
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/jotflock.png"))
        .expect("Embedded Jotflock icon is a valid PNG");
    eframe::run_native(
        "Jotflock",
        eframe::NativeOptions {
            vsync: !cfg!(target_os = "linux"),
            viewport: egui::ViewportBuilder::default()
                .with_app_id("jotflock")
                .with_icon(icon)
                .with_inner_size([1280., 820.])
                .with_min_inner_size([370., 280.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            Ok(Box::new(Canvas::new(
                root,
                instance,
                Some(&cc.egui_ctx),
                link,
                true,
            )?))
        }),
    )
}

#[cfg(windows)]
fn attach_parent_console(args: &[String]) {
    let console_mode = matches!(
        args.first().map(String::as_str),
        Some("cli" | "--headless" | "--help" | "-h")
    ) || args.iter().any(|arg| arg == "--install-desktop");

    if console_mode {
        unsafe {
            let _ = windows_sys::Win32::System::Console::AttachConsole(
                windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
            );
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console(_: &[String]) {}

fn cli(args: &[String]) -> Result<desktop::Reply> {
    if args.is_empty() || (args.len() == 1 && (args[0] == "help" || args[0] == "--help")) {
        return Ok(Ok(serde_json::json!(command::USAGE)).into());
    }
    let root = storage::root()?;
    match desktop::instance(&root, None)? {
        Some(instance) => {
            let mut app = Canvas::new(root, instance, None, None, false)?;
            Ok(app.command_reply(args, false))
        }
        None => desktop::send(&root, args),
    }
}

fn headless(args: &[String]) -> Result<()> {
    ensure!(cfg!(unix), "Persistent headless mode requires Unix sockets");
    ensure!(
        args.len() <= 1
            && args
                .first()
                .is_none_or(|link| link.starts_with(sharing::PREFIX)),
        "Usage: jotflock --headless [INVITATION]"
    );
    let root = storage::root()?;
    let instance = desktop::instance(&root, None)?
        .ok_or_else(|| anyhow::anyhow!("Jotflock is already running for this data directory"))?;
    let mut app = Canvas::new(root, instance, None, args.first().cloned(), true)?;
    while !app.quit {
        app.poll(None);
        app.tick();
        if !app.quit {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    Ok(())
}
