use crate::interface::{BORDER, INK, MUTED, VIOLET};
use eframe::egui::{self, Color32, Key, RichText};
use std::collections::VecDeque;

struct Entry {
    line: String,
    output: Option<(String, bool)>,
    recall: bool,
}

#[derive(Default)]
pub struct Terminal {
    input: String,
    entries: VecDeque<Entry>,
    expanded: bool,
    cursor: Option<usize>,
    draft: String,
}

impl Terminal {
    pub fn record(&mut self, line: String) {
        self.push(Entry {
            line,
            output: None,
            recall: true,
        });
    }

    pub fn history(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.line.as_str())
    }

    pub fn reply(&mut self, line: String, reply: &crate::desktop::Reply) {
        let mut output = reply.pretty();
        if output.len() > 12_000 {
            output.truncate(output.floor_char_boundary(12_000));
            output.push_str("\n…");
        }
        self.push(Entry {
            line,
            output: Some((output, !reply.ok())),
            recall: true,
        });
        self.expanded = true;
    }

    fn push(&mut self, mut entry: Entry) {
        if entry.line.split_whitespace().take(2).eq(["share", "join"]) {
            entry.line = "share join <invitation>".into();
            entry.recall = false;
        }
        if self.entries.len() == 100 {
            self.entries.pop_front();
            self.cursor = self.cursor.and_then(|i| i.checked_sub(1));
        }
        self.entries.push_back(entry);
    }

    fn recall(&mut self, previous: bool) {
        if previous && self.cursor.is_none() {
            self.draft.clone_from(&self.input);
        }
        let next = if previous {
            let end = self.cursor.unwrap_or(self.entries.len());
            (0..end)
                .rev()
                .find(|i| self.entries[*i].recall)
                .or(self.cursor)
        } else if let Some(i) = self.cursor {
            (i + 1..self.entries.len()).find(|i| self.entries[*i].recall)
        } else {
            return;
        };
        self.cursor = next;
        self.input = next.map_or_else(|| self.draft.clone(), |i| self.entries[i].line.clone());
    }

    pub fn show(&mut self, ctx: &egui::Context, home: bool, enabled: bool) -> Option<String> {
        let id = egui::Id::new("command-input");
        let focus = enabled && ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, Key::K));
        if focus {
            ctx.memory_mut(|m| m.request_focus(id));
        }
        if enabled && ctx.memory(|m| m.has_focus(id)) {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
                self.recall(true);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
                self.recall(false);
            }
        }
        let width = (ctx.screen_rect().width() - 70.).clamp(130., 560.);
        let mut submitted = None;
        egui::Area::new(egui::Id::new("command-bar"))
            .anchor(
                egui::Align2::CENTER_BOTTOM,
                [0., if home { -14. } else { -46. }],
            )
            .default_width(width + 20.)
            .interactable(enabled)
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                if !enabled {
                    ui.disable();
                }
                ui.spacing_mut().item_spacing.y = 5.;
                if self.expanded && enabled {
                    egui::Frame::popup(ui.style())
                        .fill(Color32::WHITE)
                        .show(ui, |ui| {
                            ui.set_width(width + 8.);
                            ui.horizontal(|ui| {
                                ui.weak("Command history");
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button("Copy latest command").clicked() {
                                            if let Some(entry) =
                                                self.entries.iter().rfind(|e| e.recall)
                                            {
                                                ui.output_mut(|o| {
                                                    o.copied_text = entry.line.clone()
                                                });
                                            }
                                        }
                                    },
                                );
                            });
                            egui::ScrollArea::vertical()
                                .max_height((ctx.screen_rect().height() * 0.32).min(250.))
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for entry in &self.entries {
                                        ui.label(
                                            RichText::new(format!("› {}", entry.line))
                                                .monospace()
                                                .color(INK),
                                        );
                                        if let Some((output, error)) = &entry.output {
                                            let color =
                                                if *error { Color32::DARK_RED } else { MUTED };
                                            ui.label(
                                                RichText::new(output).monospace().color(color),
                                            );
                                        }
                                    }
                                });
                        });
                }
                egui::Frame::none()
                    .fill(Color32::WHITE)
                    .stroke(egui::Stroke::new(1_f32, BORDER))
                    .rounding(8.)
                    .shadow(egui::epaint::Shadow {
                        offset: egui::vec2(0., 2.),
                        blur: 10.,
                        spread: 0.,
                        color: Color32::from_black_alpha(26),
                    })
                    .inner_margin(egui::Margin::symmetric(10., 5.))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("›").size(21.).color(VIOLET));
                            let hint = if ctx.memory(|m| m.has_focus(id)) {
                                "Type help for commands"
                            } else {
                                self.entries
                                    .back()
                                    .map_or("Enter a command · Ctrl+K", |e| e.line.as_str())
                            };
                            let input = ui.add(
                                egui::TextEdit::singleline(&mut self.input)
                                    .id(id)
                                    .desired_width((ui.available_width() - 42.).max(80.))
                                    .font(egui::TextStyle::Monospace)
                                    .hint_text(hint)
                                    .char_limit(128 * 1024)
                                    .frame(false),
                            );
                            if input.lost_focus() && ctx.input(|i| i.key_pressed(Key::Enter)) {
                                let line = std::mem::take(&mut self.input);
                                if !line.trim().is_empty() {
                                    submitted = Some(line);
                                }
                                self.cursor = None;
                                self.draft.clear();
                                input.request_focus();
                            }
                            let toggle = if self.expanded { "v" } else { "^" };
                            if ui.button(toggle).on_hover_text("Command history").clicked() {
                                self.expanded = !self.expanded;
                            }
                        });
                    });
            });
        submitted
    }
}
