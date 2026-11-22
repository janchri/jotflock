//! Desktop layout, dialogs and canvas rendering.
use crate::{
    command::{self, Edit},
    desktop,
    document::Kind,
    interface::{
        self, bold, dialog, heading, icon_button, primary, Icon, Preview, BLUE, BORDER, INK, MUTED,
        PAPER, VIOLET,
    },
    Canvas, Dialog,
};
use eframe::egui::{self, Color32};
use std::time::Duration;

impl Canvas {
    fn wordmark(ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.label(egui::RichText::new("jot").font(bold(25.)).color(VIOLET));
            ui.label(egui::RichText::new("flock").size(25.).color(INK));
            ui.add_space(3.);
            let (mark, painter) = ui.allocate_painter(egui::Vec2::splat(23.), egui::Sense::hover());
            let scale = mark.rect.width() / 54.;
            for (x, y, radius) in [(11., 13., 9.), (33., 45., 7.), (47., 7., 5.)] {
                let center = mark.rect.min + egui::vec2(x, y) * scale;
                painter.circle_filled(center, radius * scale, INK);
            }
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
    fn share_gui(&mut self, index: usize) {
        let id = self.workspace.entries[index].board.id();
        match self.share(index, true) {
            Ok(()) => self
                .terminal
                .record(command::format(&["share", "invite", &id])),
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
        self.save_before_close(ctx);
        ctx.request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.save();
    }
}
