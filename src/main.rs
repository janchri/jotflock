mod card;
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
mod workspace;

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
}
impl Canvas {
    fn new(cc: &eframe::CreationContext<'_>, instance: desktop::Instance) -> anyhow::Result<Self> {
        let preferences = storage::root()?.join("editor-name.txt");
        interface::theme(&cc.egui_ctx);
        Ok(Self {
            workspace: Workspace::load(storage::root()?)?,
            scene: scene::Scene::default(),
            status: "Saved locally".into(),
            last_save: Instant::now(),
            dialog: None,
            previews: HashMap::new(),
            template_previews: (0..templates::NAMES.len())
                .map(|index| {
                    let mut board = document::Board::new()?;
                    templates::insert(&mut board, index)?;
                    Ok(Preview::new(&board))
                })
                .collect::<anyhow::Result<_>>()?,
            network: network::Network::start(storage::root()?),
            network_status: "Sharing is off".into(),
            join_link: std::env::args()
                .skip(1)
                .find(|s| s.starts_with(sharing::PREFIX))
                .unwrap_or_default(),
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
    fn save(&mut self) {
        match self.workspace.flush() {
            Ok(()) => {
                self.status = "Saved locally".into();
                self.last_save = Instant::now();
            }
            Err(e) => self.status = format!("Save failed: {e:#}"),
        }
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
    fn lifecycle(&mut self, result: anyhow::Result<()>) {
        match result {
            Ok(()) => {
                self.reset_view();
                self.status = "Saved locally".into();
            }
            Err(e) => self.status = format!("Error: {e:#}"),
        }
    }
    fn add(&mut self, kind: Kind, text: &str, x: f32, y: f32) {
        if let Some(board) = self.workspace.board_mut() {
            match board.add(kind, text, x, y) {
                Ok(id) => {
                    self.scene.selected = Some(id);
                    self.scene.editing = None;
                    self.result(Ok(()));
                }
                Err(error) => self.result(Err(error)),
            }
        }
    }
    fn join_board(&mut self) {
        let result = (|| -> anyhow::Result<()> {
            let ticket = sharing::Invitation::parse(&self.join_link, sharing::now())?;
            self.workspace.flush()?;
            let local = self
                .workspace
                .entries
                .iter_mut()
                .find(|e| e.board.id() == ticket.body.board)
                .map(|e| e.board.save());
            self.network.join(ticket, local)?;
            self.joining = true;
            self.network_error = None;
            self.network_status = "Waiting for an online participant…".into();
            Ok(())
        })();
        if let Err(e) = result {
            self.network_status = e.to_string();
        }
    }
    fn poll_network(&mut self, ctx: &egui::Context) {
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
                    self.network.epoch += 1;
                    self.joining = false;
                }
                network::Event::Failed(epoch, text, terminal)
                    if epoch == 0 || epoch == self.network.epoch =>
                {
                    self.network_error = Some(text);
                    if terminal {
                        let _ = self.network.stop();
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
                    ctx.output_mut(|o| o.copied_text = link);
                    self.copied_for = self.network.board.clone();
                    self.copied_at = Instant::now();
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
                                    self.scene.fit_on_show = true;
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
    fn template(&mut self, index: usize) {
        if let Some(board) = self.workspace.board_mut() {
            let previous: std::collections::HashSet<_> =
                board.cards().into_iter().map(|c| c.id).collect();
            let result = templates::insert(board, index);
            match result {
                Ok(id) => {
                    self.scene.selected = Some(id);
                    self.scene.editing = None;
                    let added: Vec<_> = board
                        .cards()
                        .into_iter()
                        .filter(|c| !previous.contains(&c.id))
                        .collect();
                    self.scene.fit_cards(&added);
                    self.result(Ok(()));
                }
                Err(e) => self.result(Err(e)),
            }
        }
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
            if self.workspace.active == Some(index) {
                let result = self.workspace.entries[index].board.rename(&title);
                self.result(result);
            } else if let Err(error) = self.workspace.rename(index, &title) {
                self.status = format!("Rename failed: {error}");
            }
        }
    }

    fn open_board(&mut self, index: usize) {
        if self.workspace.active == Some(index) {
            self.home = false;
        } else {
            let result = self.workspace.open(index);
            self.home = result.is_err();
            self.lifecycle(result);
        }
    }

    fn library_actions(&mut self, ui: &mut egui::Ui) {
        if primary(ui, "+  New board").clicked() {
            let result = self.workspace.create();
            self.home = result.is_err();
            self.lifecycle(result);
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
                    self.share(index);
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
            self.open_board(index);
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
            let result = self.workspace.create_board(Some(index));
            self.home = result.is_err();
            self.lifecycle(result);
            self.scene.fit_on_show = !self.home;
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
                    self.join_board();
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
                self.network_status = match desktop::install() {
                    Ok(()) => "Desktop invitation links enabled".into(),
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
            self.template(index);
        }
        if closed || choice.is_some() {
            self.dialog = None;
        }
    }
    fn share(&mut self, index: usize) {
        let id = self.workspace.entries[index].board.id();
        self.copied_for = None;
        let result = (|| -> anyhow::Result<()> {
            if self.workspace.active != Some(index) {
                self.workspace.open(index)?;
                self.reset_view();
            }
            self.network
                .share(id.clone(), self.workspace.entries[index].board.save())?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.joining = false;
                self.network_error = None;
                self.network_status = "Preparing invitation…".into();
            }
            Err(error) => self.network_error = Some(error.to_string()),
        }
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
            if ui
                .add(
                    egui::TextEdit::singleline(&mut self.display_name)
                        .desired_width(f32::INFINITY)
                        .char_limit(64),
                )
                .changed()
            {
                if let Err(error) =
                    storage::write_atomic(&self.preferences, self.display_name.as_bytes())
                {
                    self.status = format!("Could not save editor name: {error}");
                }
            }
            ui.add_space(8.);
            ui.separator();
            heading(ui, "Share this board", 18.);
            ui.horizontal_wrapped(|ui| {
                if primary(ui, "Copy invitation").clicked() {
                    self.share(index);
                }
                if self.network.board.as_ref() == Some(&id) && ui.button("Stop sharing").clicked() {
                    match self.network.stop() {
                        Ok(()) => {
                            self.joining = false;
                            self.copied_for = None;
                            self.network_status = "Sharing is off".into();
                        }
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
                    let result = self.workspace.close(index);
                    close_settings = result.is_ok();
                    self.lifecycle(result);
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
            match self.workspace.delete(index) {
                Ok(path) => {
                    self.previews.remove(&id);
                    self.reset_view();
                    self.status = format!("Board moved to {}", path.display());
                }
                Err(e) => self.status = format!("Delete failed: {e:#}"),
            }
            self.dialog = None;
        }
    }
}
impl eframe::App for Canvas {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(link) = self.instance.links.try_recv() {
            self.join_link = link;
            self.start_link = true;
        }
        self.poll_network(ctx);
        if self.copied_at.elapsed() > Duration::from_secs(4) {
            self.copied_for = None;
        }
        if self.network.board.is_none() {
            self.scene.remote.clear();
        }
        if self.start_link {
            self.start_link = false;
            if !self.join_link.is_empty() {
                self.dialog = Some(Dialog::Join);
                self.join_board();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.scene.cancel_edit();
            self.dialog = None;
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
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
                            self.home = true;
                            self.scene.cancel_edit();
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
                                if let Some(board) = self.workspace.board() {
                                    self.scene.fit(board);
                                }
                            }
                            if icon_button(ui, Icon::Plus, "Zoom in").clicked() {
                                self.scene.zoom_center(1.2);
                            }
                            ui.label(format!("{:.0}%", self.scene.zoom * 100.));
                            if icon_button(ui, Icon::Minus, "Zoom out").clicked() {
                                self.scene.zoom_center(1. / 1.2);
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
                                self.add(
                                    kind,
                                    initial,
                                    (60. - self.scene.pan.x) / self.scene.zoom,
                                    (60. - self.scene.pan.y) / self.scene.zoom,
                                );
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
        if self.network.board.is_some()
            && !self.joining
            && self.workspace.board().map(document::Board::id) != self.network.board
            && self.network.stop().is_ok()
        {
            self.network_status = "Sharing stopped because the board was closed or changed".into();
        }
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
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.save();
    }
}
fn main() -> eframe::Result<()> {
    let setup = (|| -> anyhow::Result<Option<desktop::Instance>> {
        if std::env::args().any(|s| s == "--install-desktop") {
            desktop::install()?;
            return Ok(None);
        }
        let link = std::env::args()
            .skip(1)
            .find(|s| s.starts_with(sharing::PREFIX));
        desktop::instance(&storage::root()?, link.as_deref())
    })();
    let instance = match setup {
        Ok(Some(instance)) => instance,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("{error:#}");
            return Ok(());
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
        Box::new(move |cc| Ok(Box::new(Canvas::new(cc, instance)?))),
    )
}
