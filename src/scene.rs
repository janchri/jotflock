//! Canvas controller: transient UI state and document commands, never disk I/O.
use crate::{
    card,
    command::{self, Edit},
    document::{Board, Card, Kind},
    geometry::{segment_distance, Shape},
    interface::{icon_button, Icon},
};
use anyhow::{ensure, Result};
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

pub struct RemotePresence {
    pub name: String,
    pub cursor: Option<[f32; 2]>,
    pub selected: Option<String>,
    pub color: Color32,
}
pub struct Scene {
    pub pan: Vec2,
    pub zoom: f32,
    pub selected: Option<String>,
    pub editing: Option<String>,
    pub connecting: Option<String>,
    viewport: Vec2,
    pub fit_on_show: bool,
    pub pointer: Option<[f32; 2]>,
    pub remote: Vec<RemotePresence>,
    pub commands: Vec<String>,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            pan: Vec2::ZERO,
            zoom: 1.,
            selected: None,
            editing: None,
            connecting: None,
            viewport: Vec2::new(900., 600.),
            fit_on_show: false,
            pointer: None,
            remote: vec![],
            commands: vec![],
        }
    }
}
impl Scene {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn cancel_edit(&mut self) {
        self.editing = None;
        self.connecting = None;
    }
    pub fn select(&mut self, id: Option<String>) {
        self.selected = id;
        self.cancel_edit();
    }
    fn select_gui(&mut self, id: Option<String>) {
        if self.selected != id {
            self.commands.push(match &id {
                Some(id) => command::format(&["object", "select", id]),
                None => "object select".into(),
            });
            self.select(id);
        }
    }
    pub fn zoom_center(&mut self, factor: f32) -> Result<bool> {
        self.zoom_at(self.viewport * 0.5, factor)
    }
    pub fn zoom_at(&mut self, anchor: Vec2, factor: f32) -> Result<bool> {
        ensure!(
            factor.is_finite() && factor > 0. && anchor.is_finite(),
            "Invalid zoom input"
        );
        let next = (self.zoom * factor).clamp(0.1, 2.);
        if next == self.zoom {
            return Ok(false);
        }
        let pan = anchor - (anchor - self.pan) * (next / self.zoom);
        ensure!(
            next.is_finite() && pan.x.is_finite() && pan.y.is_finite(),
            "View is outside the coordinate range"
        );
        self.pan = pan;
        self.zoom = next;
        Ok(true)
    }
    pub fn pan_by(&mut self, delta: Vec2) -> Result<()> {
        let next = self.pan + delta;
        ensure!(
            next.x.is_finite() && next.y.is_finite(),
            "View is outside the coordinate range"
        );
        self.pan = next;
        Ok(())
    }
    pub fn finish_geometry(&mut self, board: &mut Board, card: &Card) -> Result<()> {
        if card.kind != Kind::Frame && !card.layout_header {
            board.reparent(&card.id)?;
        }
        Ok(())
    }
    fn edit_gui(&mut self, board: &mut Board, edit: Edit<'_>) -> Result<()> {
        edit.apply(self, board)?;
        self.commands.push(edit.line());
        Ok(())
    }
    pub fn fit(&mut self, board: &Board) {
        self.fit_cards(&board.cards());
    }
    pub fn fit_cards(&mut self, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        let min_x = cards
            .iter()
            .map(|c| c.x as f64)
            .fold(f64::INFINITY, f64::min);
        let min_y = cards
            .iter()
            .map(|c| c.y as f64)
            .fold(f64::INFINITY, f64::min);
        let max_x = cards
            .iter()
            .map(|c| c.x as f64 + c.width as f64)
            .fold(f64::NEG_INFINITY, f64::max);
        let max_y = cards
            .iter()
            .map(|c| c.y as f64 + c.height as f64)
            .fold(f64::NEG_INFINITY, f64::max);
        let zoom = ((self.viewport.x as f64 - 60.) / (max_x - min_x).max(1.))
            .min((self.viewport.y as f64 - 60.) / (max_y - min_y).max(1.))
            .clamp(0.1, 2.);
        self.zoom = zoom as f32;
        self.pan = egui::vec2((30. - min_x * zoom) as f32, (30. - min_y * zoom) as f32);
    }
    fn screen(&self, world: [f32; 2], origin: Pos2) -> Pos2 {
        origin + self.pan + Vec2::new(world[0], world[1]) * self.zoom
    }
    fn handle(
        &mut self,
        ui: &egui::Ui,
        board: &mut Board,
        card: &Card,
        action: card::Action,
    ) -> Result<bool> {
        let mut changed = false;
        if action.clicked {
            if let Some(from) = self.connecting.clone() {
                if from != card.id && card.kind != Kind::Frame {
                    self.edit_gui(board, Edit::Connect(&from, &card.id, ""))?;
                    return Ok(true);
                }
            }
        }
        if action.select {
            if self.connecting.is_none() {
                self.select_gui(Some(card.id.clone()));
            } else {
                self.selected = Some(card.id.clone());
                self.editing = None;
            }
        }
        if action.edit && self.connecting.is_none() {
            self.editing = Some(card.id.clone());
            ui.ctx()
                .memory_mut(|m| m.request_focus(card::text_id(card)));
        }
        if action.finished_edit && self.editing.as_ref() == Some(&card.id) {
            self.editing = None;
        }
        if action.delete {
            self.edit_gui(board, Edit::Delete(&card.id))?;
            return Ok(true);
        }
        if action.changed {
            if action.text_changed {
                self.edit_gui(board, Edit::Text(&card.id, &card.text))?;
            } else {
                board.edit(card)?;
            }
            changed = true;
        }
        if (action.move_finished || action.resize_finished) && !card.layout_header {
            self.finish_geometry(board, card)?;
            let edit = if action.resize_finished {
                Edit::Resize(&card.id, card.width, card.height)
            } else {
                Edit::Move(&card.id, card.x, card.y)
            };
            self.commands.push(edit.line());
            changed |= card.kind != Kind::Frame;
        }
        Ok(changed)
    }
    pub fn inspector(&mut self, ui: &mut egui::Ui, board: &mut Board) -> Result<bool> {
        let Some(id) = self.selected.clone() else {
            return Ok(false);
        };
        if let Some(mut card) = board.cards().into_iter().find(|c| c.id == id) {
            ui.label(card.kind.name());
            if card.layout_header {
                if ui.button("Select story map").clicked() {
                    self.select_gui(card.parent.clone());
                }
                return Ok(false);
            }
            let mut changed = false;
            if card.kind == Kind::Component {
                let mut shape = card.shape;
                ui.horizontal(|ui| {
                    for value in Shape::ALL {
                        let icon = match value {
                            Shape::Rectangle => Icon::Rectangle,
                            Shape::Ellipse => Icon::Ellipse,
                            Shape::Diamond => Icon::Diamond,
                        };
                        let response = icon_button(ui, icon, value.name());
                        if card.shape == value {
                            ui.painter().rect_stroke(
                                response.rect,
                                8.,
                                Stroke::new(1.5_f32, crate::interface::VIOLET),
                            );
                        }
                        if response.clicked() {
                            shape = value;
                        }
                    }
                });
                if shape != card.shape {
                    self.edit_gui(board, Edit::Shape(&id, shape))?;
                    changed = true;
                }
            }
            let mut resized = false;
            ui.add_enabled_ui(card.story.is_none(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("W");
                    resized |= ui
                        .add(
                            egui::DragValue::new(&mut card.width)
                                .range(120.0..=20_000.0)
                                .speed(1.),
                        )
                        .changed();
                    ui.label("H");
                    resized |= ui
                        .add(
                            egui::DragValue::new(&mut card.height)
                                .range(80.0..=20_000.0)
                                .speed(1.),
                        )
                        .changed();
                });
            });
            if resized {
                self.edit_gui(board, Edit::Resize(&id, card.width, card.height))?;
                changed = true;
            }
            if let Some(parent) = &card.parent {
                if let Some(frame) = board.cards().into_iter().find(|c| &c.id == parent) {
                    ui.label(format!("In frame: {}", frame.text));
                }
            }
            if let Some(axes) = board.story_axes(&id) {
                ui.label("Story map");
                if ui.button("+ Activity column").clicked() {
                    self.edit_gui(board, Edit::AxisAdd(&id, true, "New activity"))?;
                    changed = true;
                }
                if ui.button("+ Release lane").clicked() {
                    self.edit_gui(board, Edit::AxisAdd(&id, false, "New release"))?;
                    changed = true;
                }
                let labels: std::collections::HashMap<_, _> =
                    board.cards().into_iter().map(|c| (c.id, c.text)).collect();
                for (column, ids) in [(true, axes.columns), (false, axes.lanes)] {
                    for (i, header) in ids.iter().enumerate() {
                        ui.horizontal(|ui| -> Result<()> {
                            ui.label(labels.get(header).map(String::as_str).unwrap_or("Untitled"));
                            for (label, delta) in [("↑", -1), ("↓", 1)] {
                                if ui.small_button(label).clicked() {
                                    self.edit_gui(board, Edit::AxisMove(&id, column, i, delta))?;
                                    changed = true;
                                }
                            }
                            Ok(())
                        })
                        .inner?;
                    }
                }
            }
            ui.horizontal(|ui| -> Result<()> {
                if card.kind != Kind::Frame
                    && icon_button(ui, Icon::Connect, "Connect to another object").clicked()
                {
                    self.connecting = Some(id.clone());
                    self.editing = None;
                }
                if icon_button(
                    ui,
                    Icon::Trash,
                    if card.kind == Kind::Frame {
                        "Delete frame, keep contents"
                    } else {
                        "Delete object"
                    },
                )
                .clicked()
                {
                    self.edit_gui(board, Edit::Delete(&id))?;
                    changed = true;
                }
                Ok(())
            })
            .inner?;
            Ok(changed)
        } else if let Some(edge) = board.connectors().into_iter().find(|e| e.id == id) {
            ui.label("Directed connector");
            ui.label("Label");
            let mut text = edge.text;
            let mut changed = false;
            if ui
                .add(
                    egui::TextEdit::singleline(&mut text)
                        .id_source((&id, "label"))
                        .char_limit(8000),
                )
                .changed()
            {
                self.edit_gui(board, Edit::Text(&id, &text))?;
                changed = true;
            }
            if icon_button(ui, Icon::Trash, "Delete connector").clicked() {
                self.edit_gui(board, Edit::Delete(&id))?;
                changed = true;
            }
            Ok(changed)
        } else {
            self.selected = None;
            Ok(false)
        }
    }
    pub fn show(&mut self, ui: &mut egui::Ui, board: &mut Board) -> Result<bool> {
        let rect = ui.max_rect();
        self.viewport = rect.size();
        if std::mem::take(&mut self.fit_on_show) {
            self.fit(board);
        }
        if let Some(pointer) = ui.input(|input| input.pointer.hover_pos()).filter(|p| {
            ui.is_enabled() && rect.contains(*p) && ui.ctx().layer_id_at(*p) == Some(ui.layer_id())
        }) {
            let factor = ui.input(|input| {
                let pinch = input.zoom_delta();
                if pinch != 1. {
                    pinch
                } else {
                    (input.raw_scroll_delta.y * 0.003).exp()
                }
            });
            let anchor = pointer - rect.min;
            if self.zoom_at(anchor, factor)? {
                self.commands.push(command::format(&[
                    "view",
                    "zoom",
                    &factor.to_string(),
                    &anchor.x.to_string(),
                    &anchor.y.to_string(),
                ]));
            }
        }
        self.pointer = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| {
                ui.is_enabled()
                    && rect.contains(*p)
                    && ui.ctx().layer_id_at(*p) == Some(ui.layer_id())
            })
            .map(|p| {
                let world = (p - rect.min - self.pan) / self.zoom;
                [world.x, world.y]
            });
        let background = ui.interact(
            rect,
            egui::Id::new("canvas-pan"),
            egui::Sense::click_and_drag(),
        );
        if ui.is_enabled()
            && ui.input(|i| {
                i.pointer.button_down(egui::PointerButton::Middle)
                    && i.pointer.interact_pos().is_some_and(|p| {
                        rect.contains(p) && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
                    })
            })
        {
            let delta = ui.input(|i| i.pointer.delta());
            if delta != Vec2::ZERO {
                self.pan_by(delta)?;
                self.commands.push(command::format(&[
                    "view",
                    "pan",
                    &delta.x.to_string(),
                    &delta.y.to_string(),
                ]));
            }
        }
        let background_clicked = background.clicked();
        let mut edge_hit = None;
        let mut changed = false;
        if ui.is_enabled()
            && self.editing.is_none()
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
        {
            if let Some(id) = self.selected.clone() {
                self.edit_gui(board, Edit::Delete(&id))?;
                changed = true;
            }
        }
        let mut step = 28. * self.zoom;
        while step < 13. {
            step *= 2.;
        }
        let offset = Vec2::new(self.pan.x.rem_euclid(step), self.pan.y.rem_euclid(step));
        for x in 0..(rect.width() / step) as usize + 1 {
            for y in 0..(rect.height() / step) as usize + 1 {
                let pos = rect.min + offset + Vec2::new(x as f32 * step, y as f32 * step);
                if rect.contains(pos) {
                    ui.painter()
                        .circle_filled(pos, 1., Color32::from_rgb(218, 210, 200));
                }
            }
        }
        let mut frames: Vec<_> = board
            .cards()
            .into_iter()
            .filter(|c| c.kind == Kind::Frame)
            .collect();
        // Larger frames first. Frames never cover child widgets, even when selected.
        frames.sort_by(|a, b| {
            (b.width * b.height)
                .total_cmp(&(a.width * a.height))
                .then(a.id.cmp(&b.id))
        });
        for mut frame in frames {
            let pos = self.screen([frame.x, frame.y], rect.min);
            let selected = self.selected.as_ref() == Some(&frame.id);
            if !selected
                && !rect.intersects(Rect::from_min_size(
                    pos,
                    Vec2::new(frame.width, frame.height) * self.zoom,
                ))
            {
                continue;
            }
            let editing = self.editing.as_ref() == Some(&frame.id);
            let action = card::show(ui, &mut frame, pos, self.zoom, selected, editing);
            changed |= self.handle(ui, board, &frame, action)?;
        }
        let mut cards: Vec<_> = board
            .cards()
            .into_iter()
            .filter(|c| c.kind != Kind::Frame)
            .collect();
        let lookup: std::collections::HashMap<_, _> =
            cards.iter().map(|c| (c.id.as_str(), c)).collect();
        for edge in board.connectors() {
            let (Some(from), Some(to)) =
                (lookup.get(edge.from.as_str()), lookup.get(edge.to.as_str()))
            else {
                continue;
            };
            let (a, b) = connector_points(from, to);
            let (start, end) = (self.screen(a, rect.min), self.screen(b, rect.min));
            let selected = self.selected.as_ref() == Some(&edge.id);
            let color = if selected {
                Color32::from_rgb(107, 70, 151)
            } else {
                Color32::from_rgb(58, 100, 155)
            };
            let stroke = Stroke::new(if selected { 2.5_f32 } else { 1.7_f32 }, color);
            draw_arrow(ui.painter(), start, end, stroke);
            let label_pos = start.lerp(end, 0.40);
            let mut label_rect = Rect::NOTHING;
            let edit_id = egui::Id::new((&edge.id, "canvas-label"));
            if self.editing.as_ref() == Some(&edge.id) {
                let mut label = edge.text.clone();
                let field = ui.put(
                    Rect::from_center_size(label_pos, egui::vec2(180., 32.)),
                    egui::TextEdit::singleline(&mut label)
                        .id(edit_id)
                        .hint_text("Connector label")
                        .char_limit(8000),
                );
                if field.changed() {
                    self.edit_gui(board, Edit::Text(&edge.id, &label))?;
                    changed = true;
                }
                if field.lost_focus() {
                    self.editing = None;
                }
            } else if !edge.text.is_empty() {
                let galley = ui.painter().layout(
                    edge.text.clone(),
                    egui::FontId::proportional(14. * self.zoom),
                    color,
                    220. * self.zoom,
                );
                label_rect = Rect::from_center_size(label_pos, galley.size() + Vec2::splat(8.));
                ui.painter()
                    .rect_filled(label_rect, 4., Color32::from_rgb(245, 239, 227));
                ui.painter()
                    .galley(label_rect.min + Vec2::splat(4.), galley, color);
            }
            if background_clicked
                && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| {
                    label_rect.contains(p)
                        || segment_distance([p.x, p.y], [start.x, start.y], [end.x, end.y]) <= 7.
                })
            {
                let edit = label_rect
                    .contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or(Pos2::ZERO));
                edge_hit = Some((edge.id, edit.then_some(edit_id)));
            }
        }
        if let Some((id, edit)) = edge_hit {
            self.select_gui(Some(id.clone()));
            if let Some(edit) = edit {
                self.editing = Some(id);
                ui.ctx().memory_mut(|m| m.request_focus(edit));
            }
        } else if background_clicked {
            self.select_gui(None);
        }
        if let Some(from) = self
            .connecting
            .as_ref()
            .and_then(|id| lookup.get(id.as_str()))
        {
            if let Some(pointer) = ui.input(|i| i.pointer.hover_pos()).filter(|p| {
                ui.is_enabled()
                    && rect.contains(*p)
                    && ui.ctx().layer_id_at(*p) == Some(ui.layer_id())
            }) {
                let start = self.screen(from.bounds().center(), rect.min);
                draw_arrow(
                    ui.painter(),
                    start,
                    pointer,
                    Stroke::new(1_f32, crate::interface::VIOLET),
                );
                ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        cards.sort_by_key(|c| self.selected.as_ref() == Some(&c.id));
        for mut card in cards {
            let selected = self.selected.as_ref() == Some(&card.id);
            let pos = self.screen([card.x, card.y], rect.min);
            if !selected
                && !rect.intersects(Rect::from_min_size(
                    pos,
                    Vec2::new(card.width, card.height) * self.zoom,
                ))
            {
                continue;
            }
            let editing = self.editing.as_ref() == Some(&card.id);
            let action = card::show(ui, &mut card, pos, self.zoom, selected, editing);
            changed |= self.handle(ui, board, &card, action)?;
        }
        let all = board.cards();
        for peer in &self.remote {
            if let Some(id) = &peer.selected {
                if let Some(c) = all.iter().find(|c| &c.id == id) {
                    ui.painter().rect_stroke(
                        Rect::from_min_size(
                            self.screen([c.x, c.y], rect.min),
                            Vec2::new(c.width, c.height) * self.zoom,
                        )
                        .expand(3.),
                        5.,
                        Stroke::new(2_f32, peer.color),
                    );
                }
            }
            if let Some(world) = peer.cursor {
                let p = self.screen(world, rect.min);
                if rect.contains(p) {
                    ui.painter().add(egui::Shape::convex_polygon(
                        vec![p, p + Vec2::new(13., 18.), p + Vec2::new(1., 13.)],
                        peer.color,
                        Stroke::NONE,
                    ));
                    ui.painter().text(
                        p + Vec2::new(15., 15.),
                        egui::Align2::LEFT_TOP,
                        &peer.name,
                        egui::FontId::proportional(13.),
                        peer.color,
                    );
                }
            }
        }
        Ok(changed)
    }
}

fn draw_arrow(painter: &egui::Painter, start: Pos2, end: Pos2, stroke: Stroke) {
    let delta = end - start;
    let tip = delta.normalized() * (delta.length() * 0.25).min(9.);
    let wing = egui::vec2(-tip.y, tip.x) * 0.48;
    painter.line_segment([start, end], stroke);
    painter.add(egui::Shape::line(
        vec![end - tip + wing, end, end - tip - wing],
        stroke,
    ));
}

pub fn connector_points(from: &Card, to: &Card) -> ([f32; 2], [f32; 2]) {
    let shape = |c: &Card| {
        if c.kind == Kind::Component {
            c.shape
        } else {
            Shape::Rectangle
        }
    };
    (
        from.bounds().anchor(to.bounds().center(), shape(from)),
        to.bounds().anchor(from.bounds().center(), shape(to)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fitting_extreme_coordinates_keeps_view_finite() -> Result<()> {
        let mut board = Board::new()?;
        board.add(Kind::Sticky, "Left", -f32::MAX, 0.)?;
        board.add(Kind::Sticky, "Right", f32::MAX, 0.)?;
        let mut scene = Scene::default();
        scene.fit(&board);
        assert!(scene.zoom.is_finite());
        assert!(scene.pan.is_finite());
        let before = (scene.zoom, scene.pan);
        for factor in [f32::NAN, f32::INFINITY, 0., -1.] {
            assert!(scene.zoom_at(Vec2::ZERO, factor).is_err());
        }
        assert_eq!((scene.zoom, scene.pan), before);
        Ok(())
    }
    fn frame(
        ctx: &egui::Context,
        scene: &mut Scene,
        board: &mut Board,
        time: f64,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(1200.))),
            time: Some(time),
            events,
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    scene.show(ui, board).unwrap();
                });
        });
    }
    #[test]
    fn pinch_and_scroll_zoom_around_pointer() -> Result<()> {
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let mut board = Board::new()?;
        let pointer = Pos2::new(340., 280.);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            0.,
            vec![egui::Event::PointerMoved(pointer), egui::Event::Zoom(1.25)],
        );
        assert!((scene.zoom - 1.25).abs() < 0.001);
        assert!((scene.screen([340., 280.], Pos2::ZERO) - pointer).length() < 0.01);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            0.1,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: Vec2::new(0., 90.),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(scene.zoom > 1.25);
        assert!((scene.screen([340., 280.], Pos2::ZERO) - pointer).length() < 0.01);
        Ok(())
    }

    fn button(pos: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }
    fn drag(ctx: &egui::Context, scene: &mut Scene, board: &mut Board, start: Pos2, end: Pos2) {
        frame(ctx, scene, board, 0., vec![]);
        frame(
            ctx,
            scene,
            board,
            0.1,
            vec![egui::Event::PointerMoved(start)],
        );
        frame(ctx, scene, board, 0.2, vec![button(start, true)]);
        frame(ctx, scene, board, 0.3, vec![egui::Event::PointerMoved(end)]);
        frame(ctx, scene, board, 0.4, vec![button(end, false)]);
    }
    #[test]
    fn dragging_frame_header_moves_members_and_connectors_follow_resize() -> Result<()> {
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let mut board = Board::new()?;
        let area = board.add(Kind::Frame, "Area", 20., 20.)?;
        let child = board.add(Kind::Component, "Child", 80., 100.)?;
        let target = board.add(Kind::Component, "Target", 900., 100.)?;
        board.reparent(&child)?;
        board.connect(&child, &target)?;
        drag(
            &ctx,
            &mut scene,
            &mut board,
            Pos2::new(40., 35.),
            Pos2::new(140., 105.),
        );
        let cards = board.cards();
        let f = cards.iter().find(|c| c.id == area).unwrap();
        let c = cards.iter().find(|c| c.id == child).unwrap();
        assert_eq!((f.x, f.y), (120., 90.));
        assert_eq!((c.x, c.y), (180., 170.));
        let t = cards.iter().find(|c| c.id == target).unwrap();
        let (before, _) = connector_points(c, t);
        let mut resized = c.clone();
        resized.width += 100.;
        board.edit(&resized)?;
        let (after, _) = connector_points(&resized, t);
        assert!(after[0] > before[0]);
        Ok(())
    }
    #[test]
    fn selected_resize_handle_changes_geometry_without_moving_object() -> Result<()> {
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let mut board = Board::new()?;
        let id = board.add(Kind::Component, "Resize", 40., 60.)?;
        scene.selected = Some(id);
        drag(
            &ctx,
            &mut scene,
            &mut board,
            Pos2::new(267., 213.),
            Pos2::new(347., 253.),
        );
        let c = board.cards().remove(0);
        assert_eq!((c.x, c.y), (40., 60.));
        assert_eq!((c.width, c.height), (312., 198.));
        Ok(())
    }
    #[test]
    fn click_target_creates_connection_and_line_can_be_selected() -> Result<()> {
        let ctx = egui::Context::default();
        let mut scene = Scene::default();
        let mut board = Board::new()?;
        let a = board.add(Kind::Component, "A", 40., 60.)?;
        let b = board.add(Kind::Component, "B", 500., 60.)?;
        scene.connecting = Some(a.clone());
        frame(&ctx, &mut scene, &mut board, 0., vec![]);
        let p = Pos2::new(550., 130.);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            0.1,
            vec![egui::Event::PointerMoved(p)],
        );
        frame(&ctx, &mut scene, &mut board, 0.2, vec![button(p, true)]);
        frame(&ctx, &mut scene, &mut board, 0.3, vec![button(p, false)]);
        let edge = board.connectors().remove(0);
        assert_eq!((edge.from, edge.to), (a, b));
        assert!(scene.connecting.is_none());
        let p = Pos2::new(380., 139.);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            0.8,
            vec![egui::Event::PointerMoved(p)],
        );
        frame(&ctx, &mut scene, &mut board, 0.9, vec![button(p, true)]);
        frame(&ctx, &mut scene, &mut board, 1., vec![button(p, false)]);
        assert_eq!(scene.selected.as_ref(), Some(&edge.id));
        board.label_connector(&edge.id, "API")?;
        let label = Pos2::new(363., 139.);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            1.5,
            vec![egui::Event::PointerMoved(label)],
        );
        frame(&ctx, &mut scene, &mut board, 1.6, vec![button(label, true)]);
        frame(
            &ctx,
            &mut scene,
            &mut board,
            1.7,
            vec![button(label, false)],
        );
        assert_eq!(scene.editing.as_ref(), Some(&edge.id));
        frame(
            &ctx,
            &mut scene,
            &mut board,
            1.8,
            vec![egui::Event::Text(" call".into())],
        );
        assert_eq!(board.connectors()[0].text, "API call");
        Ok(())
    }
    #[test]
    fn dropping_object_into_frame_adopts_it_and_dragging_out_releases_it() -> Result<()> {
        let mut scene = Scene::default();
        let mut board = Board::new()?;
        let area = board.add(Kind::Frame, "Area", 400., 40.)?;
        let child = board.add(Kind::Sticky, "Move me", 20., 100.)?;
        drag(
            &egui::Context::default(),
            &mut scene,
            &mut board,
            Pos2::new(70., 170.),
            Pos2::new(570., 170.),
        );
        assert_eq!(
            board
                .cards()
                .iter()
                .find(|c| c.id == child)
                .unwrap()
                .parent
                .as_ref(),
            Some(&area)
        );
        drag(
            &egui::Context::default(),
            &mut scene,
            &mut board,
            Pos2::new(570., 170.),
            Pos2::new(70., 170.),
        );
        assert!(board
            .cards()
            .iter()
            .find(|c| c.id == child)
            .unwrap()
            .parent
            .is_none());
        Ok(())
    }
}
