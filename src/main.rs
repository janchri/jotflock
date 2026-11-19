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
use command::Edit;
use document::Kind;
use eframe::egui::{self, Color32};
use interface::{
    bold, dialog, heading, icon_button, primary, Icon, Preview, BLUE, BORDER, INK, MUTED, PAPER,
    VIOLET,
};
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
    fn wordmark(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.label(egui::RichText::new("jot").font(bold(25.)).color(VIOLET));
            ui.label(egui::RichText::new("flock").size(25.).color(INK));
        });
    }

    fn title(&mut self, ui: &mut egui::Ui, index: usize, width: f32, framed: bool) {
        let board = &self.workspace.entries[index].board;
        let id = board.id();
        let mut title = board.title();
        let response = ui
            .add(
                egui::TextEdit::singleline(&mut title)
                    .id(ui.make_persistent_id(("board-title", &id)))
                    .hint_text("Untitled board")
                    .desired_width(width.max(48.))
                    .font(bold(17.))
                    .frame(framed),
            )
            .on_hover_ui(|ui| {
                ui.label(&title);
                ui.weak("Click to rename");
            });
        if response.changed() {
            let result = self.rename_board(index, &title);
            self.record_gui(command::format(&["board", "rename", &id, &title]), result);
        }
    }

    fn open_board(&mut self, index: usize) -> Result<()> {
        if self.workspace.active != Some(index) {
            self.workspace.open(index)?;
            self.reset_view();
            self.status = "Saved locally".into();
        }
        self.home = false;
        Ok(())
    }

    fn library_actions(&mut self, ui: &mut egui::Ui) {
        if primary(ui, "+  New board").clicked() {
            self.run_gui(&["board", "new"]);
        }
        if ui.button("Join a board").clicked() {
            self.dialog = Some(Dialog::Join);
        }
    }

    fn board_tile(&mut self, ui: &mut egui::Ui, index: usize, width: f32) {
        let entry = &self.workspace.entries[index];
        let id = entry.board.id();
        let open = self
            .previews
            .entry(id.clone())
            .or_insert_with(|| Preview::new(&entry.board))
            .show(ui, &entry.board.title())
            .clicked();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            self.title(ui, index, (width - 96.).max(50.), false);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, Icon::More, "Board settings").clicked() {
                    self.dialog = Some(Dialog::Settings(id.clone()));
                }
                let copied = self.copied_for.as_ref() == Some(&id)
                    && self.network.board.as_ref() == Some(&id);
                let (icon, hint) = if copied {
                    (Icon::Check, "Invitation copied · Copy again")
                } else {
                    (Icon::Share, "Copy board invitation")
                };
                if icon_button(ui, icon, hint).clicked() {
                    self.share_gui(index);
                }
            });
        });
        if self.network.board.as_ref() == Some(&id) && !self.joining {
            ui.colored_label(
                BLUE,
                format!("Live · {} online", 1 + self.scene.remote.len()),
            );
        }
        if open {
            self.run_gui(&["board", "open", &id]);
        }
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        if ui.available_width() < 490. {
            heading(ui, "Your boards", 28.);
            ui.horizontal(|ui| self.library_actions(ui));
        } else {
            ui.horizontal(|ui| {
                heading(ui, "Your boards", 32.);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.library_actions(ui)
                });
            });
        }
        ui.add_space(16.);
        let width = ui.available_width();
        let columns = ((width + 14.) / 250.).floor().clamp(1., 4.) as usize;
        let tile_width = (width - (columns - 1) as f32 * 14.) / columns as f32;
        for open in [true, false] {
            let entries: Vec<_> = self
                .workspace
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.open == open)
                .map(|(index, _)| index)
                .collect();
            if entries.is_empty() && !open {
                continue;
            }
            heading(ui, if open { "Open boards" } else { "Saved boards" }, 17.);
            if entries.is_empty() {
                ui.label("No open boards");
            }
            for row in entries.chunks(columns) {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.;
                    for &index in row {
                        let id = self.workspace.entries[index].board.id();
                        ui.scope_builder(
                            egui::UiBuilder::new()
                                .id_salt(&id)
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                            |ui| {
                                egui::Frame::none()
                                    .fill(Color32::WHITE)
                                    .stroke(egui::Stroke::new(1_f32, BORDER))
                                    .rounding(12.)
                                    .inner_margin(10.)
                                    .show(ui, |ui| {
                                        ui.set_width(tile_width - 22.);
                                        self.board_tile(ui, index, ui.available_width());
                                    });
                            },
                        );
                    }
                });
            }
            ui.add_space(16.);
        }
        heading(ui, "Start from a template", 17.);
        if let Some(index) = interface::template_choices(ui, &self.template_previews) {
            self.run_gui(&["board", "new", command::template_key(index)]);
        }
        if self.status != "Saved locally" {
            ui.label(&self.status);
        }
        if self.joining {
            ui.label(&self.network_status);
        }
        if let Some(error) = &self.network_error {
            ui.colored_label(Color32::DARK_RED, error);
        }
    }

    fn join_dialog(&mut self, ctx: &egui::Context) {
        let closed = dialog(ctx, "Join a board", |ui| {
            ui.label("Invitation link");
            let link = ui.add(
                egui::TextEdit::singleline(&mut self.join_link)
                    .desired_width(f32::INFINITY)
                    .hint_text("Paste a board invitation")
                    .char_limit(128 * 1024),
            );
            let enter = link.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.add_enabled_ui(!self.join_link.trim().is_empty() && !self.joining, |ui| {
                if (primary(ui, "Join board").clicked() || enter) && ui.is_enabled() {
                    self.join_gui();
                }
            });
            if self.network_status != "Sharing is off" {
                ui.label(&self.network_status);
            }
            if let Some(error) = &self.network_error {
                ui.colored_label(Color32::DARK_RED, error);
            }
            #[cfg(target_os = "linux")]
            if ui.link("Enable invitation links on this PC").clicked() {
                self.network_status = match command::desktop_install() {
                    Ok(()) => {
                        self.terminal.record("share install".into());
                        "Desktop invitation links enabled".into()
                    }
                    Err(error) => format!("Desktop integration failed: {error}"),
                };
            }
        });
        if closed {
            self.dialog = None;
        }
    }

    fn templates_dialog(&mut self, ctx: &egui::Context) {
        let mut choice = None;
        let closed = dialog(ctx, "Insert a template", |ui| {
            choice = interface::template_choices(ui, &self.template_previews);
        });
        if let Some(index) = choice {
            self.run_gui(&["template", "insert", command::template_key(index)]);
        }
        if closed || choice.is_some() {
            self.dialog = None;
        }
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

    fn share_gui(&mut self, index: usize) {
        let id = self.workspace.entries[index].board.id();
        match self.share(index, true) {
            Ok(()) => self
                .terminal
                .record(command::format(&["share", "invite", &id])),
            Err(error) => self.network_error = Some(error.to_string()),
        }
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

    fn board_settings(&mut self, ctx: &egui::Context, id: String) {
        let Some(index) = self
            .workspace
            .entries
            .iter()
            .position(|e| e.board.id() == id)
        else {
            self.dialog = None;
            return;
        };
        let mut close_settings = false;
        let closed = dialog(ctx, "Board settings", |ui| {
            ui.label("Board name");
            self.title(ui, index, ui.available_width(), true);
            ui.label("Your editor name");
            let mut name = self.display_name.clone();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut name)
                        .desired_width(f32::INFINITY)
                        .char_limit(64),
                )
                .changed()
            {
                self.run_gui(&["editor", "name", &name]);
            }
            ui.add_space(8.);
            ui.separator();
            heading(ui, "Share this board", 18.);
            ui.horizontal_wrapped(|ui| {
                if primary(ui, "Copy invitation").clicked() {
                    self.share_gui(index);
                }
                if self.network.board.as_ref() == Some(&id) && ui.button("Stop sharing").clicked() {
                    match self.stop_sharing() {
                        Ok(()) => self.terminal.record("share stop".into()),
                        Err(error) => self.network_error = Some(error.to_string()),
                    }
                }
            });
            if self.copied_for.as_ref() == Some(&id) {
                ui.colored_label(BLUE, "Invitation copied");
            }
            ui.colored_label(MUTED, "Multiple editors · 24-hour link");
            if self.network.board.as_ref() == Some(&id) {
                ui.label(&self.network_status);
            }
            if let Some(error) = &self.network_error {
                ui.colored_label(Color32::DARK_RED, error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if self.workspace.entries[index].open && ui.button("Close board").clicked() {
                    let result = self.close_board(index);
                    close_settings = result.is_ok();
                    self.record_gui(command::format(&["board", "close", &id]), result);
                }
                if ui.button("Delete board…").clicked() {
                    self.dialog = Some(Dialog::Delete(id.clone()));
                }
            });
            if self.status != "Saved locally" {
                ui.label(&self.status);
            }
        });
        if closed || close_settings {
            self.dialog = None;
        }
    }

    fn confirm_delete(&mut self, ctx: &egui::Context, id: String) {
        let Some(index) = self
            .workspace
            .entries
            .iter()
            .position(|e| e.board.id() == id)
        else {
            self.dialog = None;
            return;
        };
        let title = self.workspace.entries[index].board.title();
        let mut cancel = false;
        let mut delete = false;
        let closed = dialog(ctx, "Move board to trash?", |ui| {
            ui.label(format!("“{title}” will leave your library. A recovery copy stays in the local trash folder."));
            ui.add_space(10.);
            ui.horizontal(|ui| {
                cancel = ui.button("Cancel").clicked();
                delete = ui
                    .button(egui::RichText::new("Move to trash").color(Color32::DARK_RED))
                    .clicked();
            });
        });
        if closed || cancel {
            self.dialog = None;
        }
        if delete {
            let result = self.delete_board(index);
            self.record_gui(command::format(&["board", "delete", &id]), result);
            self.dialog = None;
        }
    }
}
impl Canvas {
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
impl eframe::App for Canvas {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let had_dialog = self.dialog.is_some();
        self.poll(Some(ctx));
        if had_dialog && self.dialog.is_none() {
            Self::release_keyboard(ctx);
        }
        if self.quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.dialog.is_some()
                || self.scene.editing.is_some()
                || self.scene.connecting.is_some()
            {
                self.run_gui(&["view", "cancel"]);
            }
            Self::release_keyboard(ctx);
        }
        if self.workspace.board().is_none() {
            self.home = true;
        }
        let blocked = self.dialog.is_some();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::none()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(16., 9.)),
            )
            .show(ctx, |ui| {
                if blocked {
                    ui.disable();
                }
                ui.horizontal(|ui| {
                    if self.home {
                        Self::wordmark(ui);
                    } else {
                        if icon_button(ui, Icon::Home, "Return to Home").clicked() {
                            self.run_gui(&["view", "home"]);
                        }
                        ui.separator();
                        if let Some(index) = self.workspace.active {
                            let width = (ui.available_width() - 115.).clamp(56., 320.);
                            self.title(ui, index, width, false);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if self.network.board.is_some() && !self.joining {
                                ui.colored_label(
                                    BLUE,
                                    format!("{} online", self.scene.remote.len() + 1),
                                );
                            }
                        });
                    }
                });
            });
        if self.home {
            self.scene.pointer = None;
            let margin = if ctx.screen_rect().width() < 650. {
                16.
            } else {
                32.
            };
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(PAPER).inner_margin(margin))
                .show(ctx, |ui| {
                    if blocked {
                        ui.disable();
                    }
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            self.home(ui);
                            ui.add_space(44.);
                        });
                });
        } else {
            egui::TopBottomPanel::bottom("board-controls")
                .frame(
                    egui::Frame::none()
                        .fill(Color32::WHITE)
                        .inner_margin(egui::Margin::symmetric(12., 4.)),
                )
                .show(ctx, |ui| {
                    if blocked {
                        ui.disable();
                    }
                    ui.horizontal(|ui| {
                        if self.scene.selected.is_some()
                            && icon_button(ui, Icon::Sliders, "Object properties").clicked()
                        {
                            self.dialog = Some(Dialog::Properties);
                        }
                        if ui.available_width() > 400. || self.status != "Saved locally" {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(&self.status).size(13.).color(MUTED),
                                )
                                .truncate(),
                            );
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, Icon::Fit, "Fit board to window").clicked() {
                                self.run_gui(&["view", "fit"]);
                            }
                            if icon_button(ui, Icon::Plus, "Zoom in").clicked() {
                                self.run_gui(&["view", "zoom", "1.2"]);
                            }
                            ui.label(format!("{:.0}%", self.scene.zoom * 100.));
                            if icon_button(ui, Icon::Minus, "Zoom out").clicked() {
                                self.run_gui(&["view", "zoom", &(1_f32 / 1.2).to_string()]);
                            }
                        });
                    });
                });
            egui::SidePanel::left("creation-rail")
                .resizable(false)
                .exact_width(54.)
                .frame(egui::Frame::none().fill(Color32::WHITE).inner_margin(6.))
                .show(ctx, |ui| {
                    if blocked {
                        ui.disable();
                    }
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.;
                        for (kind, icon, initial) in [
                            (Kind::Sticky, Icon::Note, "Your idea…"),
                            (Kind::Component, Icon::Component, "Component"),
                            (Kind::Text, Icon::Text, "Your text…"),
                            (Kind::Frame, Icon::Frame, "Untitled frame"),
                        ] {
                            if icon_button(ui, icon, &format!("Add {}", kind.name())).clicked() {
                                let x = (60. - self.scene.pan.x) / self.scene.zoom;
                                let y = (60. - self.scene.pan.y) / self.scene.zoom;
                                let edit = Edit::Add(kind, x, y, initial);
                                let line = edit.line();
                                let result = self.edit_command(edit);
                                self.record_gui(line, result);
                            }
                        }
                        ui.separator();
                        if icon_button(ui, Icon::Templates, "Insert a template").clicked() {
                            self.dialog = Some(Dialog::Templates);
                        }
                    });
                });
            egui::CentralPanel::default()
                .frame(egui::Frame::none().fill(PAPER))
                .show(ctx, |ui| {
                    if blocked {
                        ui.disable();
                    }
                    if let Some(board) = self.workspace.board_mut() {
                        let result = self.scene.show(ui, board);
                        self.canvas_result(result);
                    }
                    if self.scene.connecting.is_some() {
                        egui::Frame::none()
                            .fill(Color32::WHITE)
                            .inner_margin(8.)
                            .rounding(8.)
                            .show(ui, |ui| {
                                ui.colored_label(
                                    VIOLET,
                                    "Click the object to connect to · Esc to cancel",
                                );
                            });
                    }
                });
        }
        match self.dialog.clone() {
            Some(Dialog::Settings(id)) => self.board_settings(ctx, id),
            Some(Dialog::Delete(id)) => self.confirm_delete(ctx, id),
            Some(Dialog::Templates) => self.templates_dialog(ctx),
            Some(Dialog::Join) => self.join_dialog(ctx),
            Some(Dialog::Properties) => {
                let closed = dialog(ctx, "Object properties", |ui| {
                    if let Some(board) = self.workspace.board_mut() {
                        let result = self.scene.inspector(ui, board);
                        self.canvas_result(result);
                    }
                });
                if closed || self.scene.selected.is_none() || self.scene.connecting.is_some() {
                    self.dialog = None;
                }
            }
            None => {}
        }
        for line in std::mem::take(&mut self.scene.commands) {
            self.terminal.record(line);
        }
        if let Some(line) = self.terminal.show(ctx, self.home, self.dialog.is_none()) {
            let parsed = command::parse(&line);
            let display = parsed.as_ref().map_or_else(
                |_| line.clone(),
                |args| command::format(&args.iter().map(String::as_str).collect::<Vec<_>>()),
            );
            let reply = match parsed {
                Ok(args) => self.command_reply(&args, true),
                Err(error) => desktop::Reply::from(Err(error)),
            };
            self.terminal.reply(display, &reply);
        }
        self.tick();
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.save();
    }
}
fn main() -> eframe::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
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
