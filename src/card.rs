use crate::{
    document::{Card, Kind},
    geometry::Shape,
    interface::{Icon, BLUE, INK, VIOLET},
};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2};

#[derive(Default)]
pub struct Action {
    pub changed: bool,
    pub delete: bool,
    pub select: bool,
    pub clicked: bool,
    pub edit: bool,
    pub finished_edit: bool,
    pub move_finished: bool,
}
pub fn text_id(card: &Card) -> egui::Id {
    egui::Id::new((&card.id, "text"))
}

/// Frames expose their header, leaving the interior available for child objects.
pub fn show(
    ui: &mut egui::Ui,
    card: &mut Card,
    pos: Pos2,
    zoom: f32,
    selected: bool,
    editing: bool,
) -> Action {
    let mut action = Action::default();
    let rect = Rect::from_min_size(pos, Vec2::new(card.width, card.height) * zoom);
    let is_frame = card.kind == Kind::Frame;
    let header = Rect::from_min_size(pos, Vec2::new(rect.width(), 34. * zoom));
    let response = ui.interact(
        if is_frame { header } else { rect },
        egui::Id::new((&card.id, "card")),
        Sense::click_and_drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(if response.dragged() {
            egui::CursorIcon::Grabbing
        } else {
            egui::CursorIcon::Grab
        });
    }
    action.clicked = response.clicked();
    action.select = response.clicked() || response.drag_started();
    action.edit = response.double_clicked();
    action.move_finished = response.drag_stopped_by(egui::PointerButton::Primary);
    if response.dragged_by(egui::PointerButton::Primary) && !card.layout_header {
        let delta = response.drag_delta() / zoom;
        card.x += delta.x;
        card.y += delta.y;
        action.changed = delta != Vec2::ZERO;
    }
    let fill = match card.kind {
        Kind::Sticky => Color32::from_rgb(253, 229, 170),
        Kind::Component => Color32::from_rgb(233, 220, 250),
        Kind::Text => Color32::TRANSPARENT,
        Kind::Frame => Color32::from_rgb(243, 245, 249),
    };

    let stroke = Stroke::new(
        if selected { 2.0_f32 } else { 1.0_f32 },
        if selected {
            VIOLET
        } else {
            match card.kind {
                Kind::Text => Color32::TRANSPARENT,
                Kind::Sticky => Color32::from_rgb(224, 192, 133),
                Kind::Frame => Color32::from_rgb(184, 202, 221),
                Kind::Component => Color32::from_rgb(164, 137, 188),
            }
        },
    );
    let shape = if card.kind == Kind::Component {
        card.shape
    } else {
        Shape::Rectangle
    };
    match shape {
        Shape::Rectangle => {
            ui.painter().rect(rect, 6. * zoom, fill, stroke);
        }
        Shape::Ellipse => {
            ui.painter().add(egui::Shape::ellipse_filled(
                rect.center(),
                rect.size() / 2.,
                fill,
            ));
            ui.painter().add(egui::Shape::ellipse_stroke(
                rect.center(),
                rect.size() / 2.,
                stroke,
            ));
        }
        Shape::Diamond => {
            ui.painter().add(egui::Shape::convex_polygon(
                vec![
                    rect.center_top(),
                    rect.right_center(),
                    rect.center_bottom(),
                    rect.left_center(),
                ],
                fill,
                stroke,
            ));
        }
    }
    if is_frame {
        ui.painter()
            .rect_filled(header, 5. * zoom, Color32::from_rgb(202, 222, 240));
        Icon::Frame.paint(
            ui.painter(),
            Rect::from_center_size(pos + Vec2::new(17., 17.) * zoom, Vec2::splat(16. * zoom)),
            BLUE,
        );
    } else if card.kind == Kind::Sticky {
        ui.painter().rect_filled(
            Rect::from_min_size(pos + Vec2::new(14., 12.) * zoom, Vec2::new(25., 3.) * zoom),
            zoom,
            Color32::from_rgb(195, 150, 71),
        );
    }
    let delete_rect = Rect::from_min_size(
        pos + Vec2::new(card.width - 28., 5.) * zoom,
        Vec2::splat(23. * zoom),
    );
    if selected && !card.layout_header {
        let delete = ui.interact(
            delete_rect,
            egui::Id::new((&card.id, "delete")),
            Sense::click(),
        );
        Icon::Close.paint(ui.painter(), delete_rect.shrink(5. * zoom), VIOLET);
        action.delete = delete
            .on_hover_text(if is_frame {
                "Delete frame, keep its contents"
            } else {
                "Delete object"
            })
            .clicked();
    }
    let body = if is_frame {
        Rect::from_min_max(
            pos + Vec2::new(34., 5.) * zoom,
            header.max - Vec2::new(36., 3.) * zoom,
        )
    } else if shape != Shape::Rectangle {
        rect.shrink2(Vec2::new(rect.width() * 0.17, rect.height() * 0.24))
    } else {
        Rect::from_min_max(
            pos + Vec2::new(14., if card.kind == Kind::Sticky { 24. } else { 12. }) * zoom,
            rect.max - Vec2::splat(12. * zoom),
        )
    };
    let font = FontId::proportional(if is_frame { 18. } else { 20. } * zoom);
    let empty = card.text.is_empty() && !editing;
    let (display, color) = if empty {
        (card.kind.name(), crate::interface::MUTED)
    } else {
        (card.text.as_str(), INK)
    };
    let galley = ui
        .painter()
        .layout(display.to_owned(), font.clone(), color, body.width());
    let text_origin = if card.kind == Kind::Component {
        body.min + ((body.size() - galley.size()) * 0.5).max(Vec2::ZERO)
    } else {
        body.min
    };
    if editing {
        let text_id = text_id(card);
        let edit = if is_frame {
            egui::TextEdit::singleline(&mut card.text)
        } else {
            egui::TextEdit::multiline(&mut card.text)
        };
        let edit_rect = Rect::from_min_max(
            text_origin,
            Pos2::new(
                body.right(),
                (text_origin.y + galley.size().y).min(body.bottom()),
            ),
        );
        let mut edit_ui = ui.new_child(egui::UiBuilder::new().max_rect(edit_rect));
        edit_ui.set_clip_rect(body.intersect(ui.clip_rect()));
        let field = edit_ui.put(
            edit_rect,
            edit.id(text_id)
                .font(font)
                .desired_rows(1)
                .margin(egui::Margin::ZERO)
                .char_limit(8000)
                .frame(false),
        );
        action.changed |= field.changed();
        action.finished_edit = field.lost_focus();
    } else {
        let label_hit = if card.text.is_empty() {
            body
        } else {
            Rect::from_min_size(text_origin, galley.size())
                .expand(4.)
                .intersect(body)
        };
        if response.hover_pos().is_some_and(|p| label_hit.contains(p)) {
            if !response.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            action.edit |= response.clicked();
        }
        if let Some(point) = response.interact_pointer_pos().filter(|_| action.edit) {
            let id = text_id(card);
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            let cursor = galley.cursor_from_pos(point - text_origin).ccursor;
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(cursor)));
            state.store(ui.ctx(), id);
        }
        ui.painter()
            .with_clip_rect(body.intersect(ui.clip_rect()))
            .galley(text_origin, galley, INK);
        response.on_hover_text(if is_frame {
            "Click the title to rename · Drag to move the frame"
        } else {
            "Click the text to edit · Drag to move"
        });
    }
    if selected && !card.layout_header && card.story.is_none() {
        let handle =
            Rect::from_min_size(rect.max - Vec2::splat(16. * zoom), Vec2::splat(16. * zoom));
        let resize = ui
            .interact(handle, egui::Id::new((&card.id, "resize")), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeNwSe);
        ui.painter().line_segment(
            [handle.left_bottom(), handle.right_top()],
            Stroke::new(2_f32, VIOLET),
        );
        if resize.dragged_by(egui::PointerButton::Primary) {
            let delta = resize.drag_delta() / zoom;
            card.width = (card.width + delta.x).clamp(if is_frame { 280. } else { 120. }, 20_000.);
            card.height = (card.height + delta.y).clamp(if is_frame { 180. } else { 80. }, 20_000.);
            action.changed = true;
        }
        action.move_finished |= resize.drag_stopped_by(egui::PointerButton::Primary);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        card: &mut Card,
        zoom: f32,
        events: Vec<egui::Event>,
        time: f64,
    ) -> Action {
        frame_edit(ctx, card, zoom, events, time, false)
    }
    fn frame_edit(
        ctx: &egui::Context,
        card: &mut Card,
        zoom: f32,
        events: Vec<egui::Event>,
        time: f64,
        editing: bool,
    ) -> Action {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::splat(900.))),
            events,
            time: Some(time),
            ..Default::default()
        };
        let mut action = Action::default();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.interact(
                    ui.max_rect(),
                    egui::Id::new("canvas-background"),
                    Sense::click_and_drag(),
                );
                action = show(
                    ui,
                    card,
                    Pos2::new(100., 100.) + Vec2::new(card.x, card.y) * zoom,
                    zoom,
                    false,
                    editing,
                );
            });
        });
        action
    }
    #[test]
    fn dragging_card_body_follows_pointer_at_all_zoom_levels() {
        for zoom in [0.4, 1., 2.] {
            for kind in [Kind::Sticky, Kind::Component] {
                let ctx = egui::Context::default();
                let mut card = Card {
                    id: "test".into(),
                    text: "Drag this text area".into(),
                    x: 0.,
                    y: 0.,
                    kind,
                    ..Default::default()
                };
                let start = Pos2::new(100., 100.) + Vec2::new(50., 70.) * zoom;
                frame(&ctx, &mut card, zoom, vec![], 0.);
                frame(
                    &ctx,
                    &mut card,
                    zoom,
                    vec![egui::Event::PointerMoved(start)],
                    0.1,
                );
                frame(
                    &ctx,
                    &mut card,
                    zoom,
                    vec![egui::Event::PointerButton {
                        pos: start,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    }],
                    0.2,
                );
                let end = start + Vec2::new(80., 40.);
                frame(
                    &ctx,
                    &mut card,
                    zoom,
                    vec![egui::Event::PointerMoved(end)],
                    0.3,
                );
                frame(
                    &ctx,
                    &mut card,
                    zoom,
                    vec![egui::Event::PointerButton {
                        pos: end,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                    0.4,
                );
                assert!(
                    (card.x - 80. / zoom).abs() < 0.01,
                    "{kind:?} at {zoom}: x={}",
                    card.x
                );
                assert!((card.y - 40. / zoom).abs() < 0.01);
            }
        }
    }
    #[test]
    fn clicking_text_once_opens_editor_and_typing_does_not_move_card() {
        let ctx = egui::Context::default();
        let mut card = Card {
            id: "edit-test".into(),
            text: "Idea".into(),
            x: 0.,
            y: 0.,
            kind: Kind::Sticky,
            ..Default::default()
        };
        let point = Pos2::new(115., 136.);
        frame(&ctx, &mut card, 1., vec![], 0.);
        frame(
            &ctx,
            &mut card,
            1.,
            vec![egui::Event::PointerMoved(point)],
            0.05,
        );
        let click = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut card, 1., vec![click(true)], 0.1);
        let action = frame(&ctx, &mut card, 1., vec![click(false)], 0.25);
        assert!(action.edit);
        ctx.memory_mut(|m| m.request_focus(text_id(&card)));
        frame_edit(&ctx, &mut card, 1., vec![], 0.3, true);
        let action = frame_edit(
            &ctx,
            &mut card,
            1.,
            vec![egui::Event::Text(" new".into())],
            0.35,
            true,
        );
        assert!(action.changed);
        assert_eq!(card.text, " newIdea");
        assert_eq!((card.x, card.y), (0., 0.));
    }
}
