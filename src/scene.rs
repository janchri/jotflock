//! Canvas controller: transient UI state and document commands, never disk I/O.
use crate::{
    card,
    document::{Board, Card, Kind},
    geometry::{segment_distance, Shape},
    interface::{icon_button, Icon},
};
use anyhow::Result;
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
    pub fn zoom_center(&mut self, factor: f32) {
        self.zoom_at(self.viewport * 0.5, factor);
    }
    fn zoom_at(&mut self, anchor: Vec2, factor: f32) {
        let next = (self.zoom * factor).clamp(0.1, 2.);
        if next.is_finite() && next != self.zoom {
            self.pan = anchor - (anchor - self.pan) * (next / self.zoom);
            self.zoom = next;
        }
    }
    pub fn fit(&mut self, board: &Board) {
        self.fit_cards(&board.cards());
    }
    pub fn fit_cards(&mut self, cards: &[Card]) {
        if cards.is_empty() {
            return;
        }
        let min = Vec2::new(
            cards.iter().map(|c| c.x).fold(f32::INFINITY, f32::min),
            cards.iter().map(|c| c.y).fold(f32::INFINITY, f32::min),
        );
        let max = Vec2::new(
            cards
                .iter()
                .map(|c| c.x + c.width)
                .fold(f32::NEG_INFINITY, f32::max),
            cards
                .iter()
                .map(|c| c.y + c.height)
                .fold(f32::NEG_INFINITY, f32::max),
        );
        let size = max - min;
        self.zoom = ((self.viewport.x - 60.) / size.x.max(1.))
            .min((self.viewport.y - 60.) / size.y.max(1.))
            .clamp(0.1, 2.);
        self.pan = Vec2::splat(30.) - min * self.zoom;
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
                    self.selected = Some(board.connect(&from, &card.id)?);
                    self.connecting = None;
                    self.editing = None;
                    return Ok(true);
                }
            }
        }
        if action.select {
            self.selected = Some(card.id.clone());
            if self.editing.as_ref() != Some(&card.id) {
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
            board.delete(&card.id)?;
            if self.selected.as_ref() == Some(&card.id) {
                self.selected = None;
                self.editing = None;
            }
            if self.connecting.as_ref() == Some(&card.id) {
                self.connecting = None;
            }
            return Ok(true);
        }
        if action.changed {
            board.edit(card)?;
            changed = true;
        }
        if action.move_finished && card.kind != Kind::Frame && !card.layout_header {
            board.reparent(&card.id)?;
            changed = true;
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
                    self.selected = card.parent.clone();
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
                    board.set_shape(&id, shape)?;
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
                board.edit(&card)?;
                if card.kind != Kind::Frame {
                    board.reparent(&id)?;
                }
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
                    board.add_axis(&id, true, "New activity")?;
                    changed = true;
                }
                if ui.button("+ Release lane").clicked() {
                    board.add_axis(&id, false, "New release")?;
                    changed = true;
                }
                let labels: std::collections::HashMap<_, _> =
                    board.cards().into_iter().map(|c| (c.id, c.text)).collect();
                for (column, ids) in [(true, axes.columns), (false, axes.lanes)] {
                    for (i, header) in ids.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(labels.get(header).map(String::as_str).unwrap_or("Untitled"));
                            for (label, delta) in [("↑", -1), ("↓", 1)] {
                                if ui.small_button(label).clicked()
                                    && board.move_axis(&id, column, i, delta).is_ok()
                                {
                                    changed = true;
                                }
                            }
                        });
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
                    board.delete(&id)?;
                    self.selected = None;
                    self.cancel_edit();
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
                board.label_connector(&id, &text)?;
                changed = true;
            }
            if icon_button(ui, Icon::Trash, "Delete connector").clicked() {
                board.delete(&id)?;
                self.selected = None;
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
        if let Some(pointer) = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|p| ui.is_enabled() && rect.contains(*p))
        {
            let factor = ui.input(|input| {
                let pinch = input.zoom_delta();
                if pinch != 1. {
                    pinch
                } else {
                    (input.raw_scroll_delta.y * 0.003).exp()
                }
            });
            self.zoom_at(pointer - rect.min, factor);
        }
        self.pointer = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| ui.is_enabled() && rect.contains(*p))
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
                    && i.pointer.interact_pos().is_some_and(|p| rect.contains(p))
            })
        {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        let background_clicked = background.clicked();
        if background_clicked {
            self.selected = None;
            self.editing = None;
        }
        let mut changed = false;
        if ui.is_enabled()
            && self.editing.is_none()
            && !ui.ctx().wants_keyboard_input()
            && ui.input(|i| i.key_pressed(egui::Key::Delete))
        {
            if let Some(id) = self.selected.take() {
                board.delete(&id)?;
                self.connecting = None;
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
                    board.label_connector(&edge.id, &label)?;
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
                if label_rect.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or(Pos2::ZERO))
                {
                    self.editing = Some(edge.id.clone());
                    ui.ctx().memory_mut(|m| m.request_focus(edit_id));
                }
                self.selected = Some(edge.id);
            }
        }
        if let Some(from) = self
            .connecting
            .as_ref()
            .and_then(|id| lookup.get(id.as_str()))
        {
            if let Some(pointer) = ui
                .input(|i| i.pointer.hover_pos())
                .filter(|p| ui.is_enabled() && rect.contains(*p))
            {
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
