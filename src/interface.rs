use crate::{
    document::{Board, Kind},
    geometry::Shape,
};
use eframe::egui::{
    self, pos2, vec2, Align2, Color32, FontFamily, FontId, Rect, RichText, Stroke, Ui,
};

pub const INK: Color32 = Color32::from_rgb(41, 37, 55);
pub const MUTED: Color32 = Color32::from_rgb(91, 85, 105);
pub const PAPER: Color32 = Color32::from_rgb(248, 245, 239);
pub const BORDER: Color32 = Color32::from_rgb(222, 216, 226);
pub const VIOLET: Color32 = Color32::from_rgb(100, 67, 153);
pub const BLUE: Color32 = Color32::from_rgb(54, 91, 143);
const LILAC: Color32 = Color32::from_rgb(237, 229, 251);

pub fn theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, data, family) in [
        (
            "regular",
            include_bytes!("../assets/fonts/Roboto-Regular.ttf").as_slice(),
            FontFamily::Proportional,
        ),
        (
            "bold",
            include_bytes!("../assets/fonts/Roboto-Bold.ttf").as_slice(),
            FontFamily::Name("heading".into()),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), egui::FontData::from_static(data));
        fonts
            .families
            .entry(family)
            .or_default()
            .insert(0, name.into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.override_text_color = Some(INK);
    style.visuals.panel_fill = Color32::WHITE;
    style.visuals.extreme_bg_color = PAPER;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.window_rounding = 14.0.into();
    style.visuals.selection.bg_fill = Color32::from_rgb(202, 185, 237);
    style.visuals.hyperlink_color = BLUE;
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, BORDER);
    style.visuals.widgets.inactive.weak_bg_fill = PAPER;
    style.visuals.widgets.hovered.weak_bg_fill = LILAC;
    style.visuals.widgets.active.weak_bg_fill = LILAC;
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widget.rounding = 7.0.into();
    }
    style.spacing.item_spacing = vec2(8., 8.);
    style.spacing.button_padding = vec2(12., 7.);
    style.spacing.interact_size.y = 32.;
    style
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(15.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, FontId::proportional(14.));
    ctx.set_style(style);
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("heading".into()))
}

pub fn heading(ui: &mut Ui, text: &str, size: f32) {
    ui.label(RichText::new(text).font(bold(size)).color(INK));
}

pub fn primary(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(RichText::new(text).color(Color32::WHITE)).fill(VIOLET))
}

#[derive(Clone, Copy)]
pub enum Icon {
    Home,
    Note,
    Component,
    Text,
    Frame,
    Templates,
    More,
    Share,
    Check,
    Trash,
    Connect,
    Rectangle,
    Ellipse,
    Diamond,
    Fit,
    Plus,
    Minus,
    Sliders,
    Close,
}

impl Icon {
    pub fn paint(self, painter: &egui::Painter, rect: Rect, color: Color32) {
        let point = |x: f32, y: f32| rect.min + vec2(x * rect.width(), y * rect.height()) / 24.;
        let stroke = Stroke::new((rect.width() / 14.).max(1.25), color);
        let path = |points: &[(f32, f32)]| {
            painter.add(egui::Shape::line(
                points.iter().map(|&(x, y)| point(x, y)).collect(),
                stroke,
            ));
        };
        let outline = |x, y, w, h| {
            painter.rect_stroke(
                Rect::from_min_max(point(x, y), point(x + w, y + h)),
                2.,
                stroke,
            );
        };
        match self {
            Self::Home => {
                path(&[(2., 11.), (12., 3.), (22., 11.)]);
                path(&[
                    (5., 10.),
                    (5., 21.),
                    (10., 21.),
                    (10., 15.),
                    (14., 15.),
                    (14., 21.),
                    (19., 21.),
                    (19., 10.),
                ]);
            }
            Self::Note => {
                painter.add(egui::Shape::convex_polygon(
                    vec![
                        point(4., 3.),
                        point(21., 3.),
                        point(21., 15.),
                        point(15., 21.),
                        point(4., 21.),
                    ],
                    Color32::from_rgb(251, 223, 160),
                    Stroke::NONE,
                ));
                path(&[
                    (4., 3.),
                    (21., 3.),
                    (21., 15.),
                    (15., 21.),
                    (4., 21.),
                    (4., 3.),
                ]);
                path(&[(15., 21.), (15., 15.), (21., 15.)]);
            }
            Self::Component => {
                painter.rect_filled(
                    Rect::from_min_max(point(2., 3.), point(14., 13.)),
                    2.,
                    LILAC,
                );
                outline(2., 3., 12., 10.);
                outline(12., 14., 10., 7.);
                path(&[(7., 13.), (7., 18.), (12., 18.)]);
            }
            Self::Text => {
                path(&[(4., 7.), (4., 4.), (20., 4.), (20., 7.)]);
                path(&[(12., 4.), (12., 21.)]);
                path(&[(8., 21.), (16., 21.)]);
            }
            Self::Frame => {
                for (x, y, dx, dy) in [
                    (4., 4., 6., 6.),
                    (20., 4., -6., 6.),
                    (4., 20., 6., -6.),
                    (20., 20., -6., -6.),
                ] {
                    path(&[(x + dx, y), (x, y), (x, y + dy)]);
                }
            }
            Self::Templates => {
                outline(3., 3., 7., 18.);
                outline(13., 3., 8., 7.);
                outline(13., 13., 8., 8.);
            }
            Self::Share => {
                path(&[(5., 12.), (18., 5.)]);
                path(&[(5., 12.), (18., 19.)]);
                for (x, y) in [(5., 12.), (18., 5.), (18., 19.)] {
                    painter.circle(point(x, y), rect.width() / 8., Color32::WHITE, stroke);
                }
            }
            Self::Check => path(&[(3., 12.), (9., 18.), (21., 5.)]),
            Self::Trash => {
                path(&[(3., 6.), (21., 6.)]);
                path(&[(9., 5.), (9., 3.), (15., 3.), (15., 5.)]);
                path(&[(6., 7.), (7., 21.), (17., 21.), (18., 7.)]);
            }
            Self::Connect => {
                outline(2., 14., 6., 6.);
                path(&[(8., 17.), (13., 17.), (13., 7.), (21., 7.), (17., 3.)]);
                path(&[(21., 7.), (17., 11.)]);
            }
            Self::Rectangle => outline(2., 5., 20., 14.),
            Self::Ellipse => {
                painter.add(egui::Shape::ellipse_stroke(
                    rect.center(),
                    rect.size() * vec2(0.43, 0.32),
                    stroke,
                ));
            }
            Self::Diamond => path(&[(12., 2.), (22., 12.), (12., 22.), (2., 12.), (12., 2.)]),
            Self::More => {
                for x in [4., 12., 20.] {
                    painter.circle_filled(point(x, 12.), rect.width() / 12., color);
                }
            }
            Self::Fit => {
                Self::Frame.paint(painter, rect, color);
                outline(9., 9., 6., 6.);
            }
            Self::Plus => {
                path(&[(12., 4.), (12., 20.)]);
                path(&[(4., 12.), (20., 12.)]);
            }
            Self::Minus => path(&[(4., 12.), (20., 12.)]),
            Self::Sliders => {
                for (y, x) in [(6., 9.), (17., 16.)] {
                    path(&[(3., y), (x - 2., y)]);
                    path(&[(x + 2., y), (21., y)]);
                    painter.circle_stroke(point(x, y), rect.width() / 9., stroke);
                }
            }
            Self::Close => {
                path(&[(6., 6.), (18., 18.)]);
                path(&[(6., 18.), (18., 6.)]);
            }
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: Icon, label: &str) -> egui::Response {
    let size = vec2(38., 38.);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    let active = response.hovered() || response.has_focus();
    if active {
        ui.painter().rect_filled(rect, 8., LILAC);
    }
    icon.paint(
        ui.painter(),
        Rect::from_center_size(rect.center(), vec2(22., 22.)),
        if active { VIOLET } else { INK },
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

pub fn dialog(ctx: &egui::Context, name: &str, content: impl FnOnce(&mut Ui)) -> bool {
    let screen = ctx.screen_rect();
    ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("dialog-scrim"),
    ))
    .rect_filled(screen, 0., Color32::from_black_alpha(48));
    let padding = if screen.width() < 520. { 16. } else { 28. };
    let width = (screen.width() - 2. * padding - 32.).clamp(80., 510.);
    let mut dismissed = false;
    egui::Area::new(egui::Id::new(("dialog", name)))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .movable(false)
        .default_height(screen.height())
        .show(ctx, |ui| {
            egui::Frame::window(ui.style())
                .inner_margin(padding)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.horizontal(|ui| {
                        heading(ui, name, 24.);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            dismissed = icon_button(ui, Icon::Close, "Close dialog").clicked();
                        });
                    });
                    ui.add_space(8.);
                    egui::ScrollArea::vertical()
                        .max_height((screen.height() - 2. * padding - 96.).max(50.))
                        .auto_shrink([false, true])
                        .show(ui, content);
                });
        });
    dismissed
}

pub struct Preview {
    shapes: Vec<(Kind, Shape, Rect)>,
    edges: Vec<(egui::Pos2, egui::Pos2)>,
    bounds: Rect,
}

impl Preview {
    pub fn new(board: &Board) -> Self {
        let cards = board.cards();
        let by_id: std::collections::HashMap<_, _> =
            cards.iter().map(|c| (c.id.as_str(), c)).collect();
        let edges = board
            .connectors()
            .into_iter()
            .filter_map(|edge| {
                let (a, b) = crate::scene::connector_points(
                    by_id.get(edge.from.as_str())?,
                    by_id.get(edge.to.as_str())?,
                );
                Some((egui::Pos2::from(a), egui::Pos2::from(b)))
            })
            .collect();
        let mut shapes: Vec<_> = cards
            .iter()
            .map(|c| {
                (
                    c.kind,
                    c.shape,
                    Rect::from_min_size(pos2(c.x, c.y), vec2(c.width, c.height)),
                )
            })
            .collect();
        shapes.sort_by_key(|(kind, _, _)| *kind != Kind::Frame);
        let bounds = shapes
            .iter()
            .fold(Rect::NOTHING, |all, (_, _, r)| all.union(*r));
        Self {
            shapes,
            edges,
            bounds,
        }
    }

    pub fn show(&self, ui: &mut Ui, title: &str) -> egui::Response {
        let (r, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 105.), egui::Sense::click());
        self.paint(
            ui,
            r,
            if response.hovered() || response.has_focus() {
                LILAC
            } else {
                Color32::from_rgb(240, 236, 245)
            },
        );
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::WidgetType::Button,
                ui.is_enabled(),
                format!("Open {title}"),
            )
        });
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Open board")
    }

    fn paint(&self, ui: &Ui, r: Rect, fill: Color32) {
        let p = ui.painter_at(r);
        p.rect_filled(r, 8., fill);
        for x in (9..r.width() as usize).step_by(15) {
            for y in (8..r.height() as usize).step_by(15) {
                p.circle_filled(
                    r.min + vec2(x as f32, y as f32),
                    0.85,
                    Color32::from_rgb(204, 197, 215),
                );
            }
        }
        if self.shapes.is_empty() {
            Icon::Note.paint(
                &p,
                Rect::from_center_size(r.center(), vec2(34., 34.)),
                VIOLET,
            );
        } else {
            let scale = ((r.width() - 30.) / self.bounds.width())
                .min((r.height() - 20.) / self.bounds.height());
            let screen = |point| r.center() + (point - self.bounds.center()) * scale;
            let draw = |(kind, shape, bounds): &(Kind, Shape, Rect)| {
                let rect = Rect::from_center_size(
                    r.center() + (bounds.center() - self.bounds.center()) * scale,
                    bounds.size() * scale,
                );
                let fill = match kind {
                    Kind::Sticky => Color32::from_rgb(246, 214, 153),
                    Kind::Component => Color32::from_rgb(194, 165, 227),
                    Kind::Frame => Color32::from_rgb(218, 229, 244),
                    Kind::Text => Color32::WHITE,
                };
                let stroke = Stroke::new(0.8_f32, MUTED);
                match shape {
                    Shape::Rectangle => {
                        p.rect(rect, 2., fill, stroke);
                    }
                    Shape::Ellipse => {
                        p.add(egui::Shape::ellipse_filled(
                            rect.center(),
                            rect.size() * 0.5,
                            fill,
                        ));
                        p.add(egui::Shape::ellipse_stroke(
                            rect.center(),
                            rect.size() * 0.5,
                            stroke,
                        ));
                    }
                    Shape::Diamond => {
                        p.add(egui::Shape::convex_polygon(
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
            };
            let split = self
                .shapes
                .partition_point(|(kind, _, _)| *kind == Kind::Frame);
            self.shapes[..split].iter().for_each(draw);
            for (a, b) in &self.edges {
                p.line_segment([screen(*a), screen(*b)], Stroke::new(0.9_f32, BLUE));
            }
            self.shapes[split..].iter().for_each(draw);
        }
    }
}

pub fn template_choices(ui: &mut Ui, previews: &[Preview]) -> Option<usize> {
    let mut choice = None;
    let width = ui.available_width();
    let columns = if width >= 610. { 3 } else { 1 };
    let tile_width = (width - ui.spacing().item_spacing.x * (columns - 1) as f32) / columns as f32;
    let colors = [
        Color32::from_rgb(249, 237, 210),
        LILAC,
        Color32::from_rgb(223, 236, 249),
    ];
    for row in (0..previews.len()).collect::<Vec<_>>().chunks(columns) {
        ui.horizontal(|ui| {
            for &i in row {
                let name = crate::templates::NAMES[i];
                let (rect, response) =
                    ui.allocate_exact_size(vec2(tile_width, 76.), egui::Sense::click());
                let p = ui.painter_at(rect);
                p.rect(
                    rect.shrink(1.),
                    9.,
                    Color32::WHITE,
                    Stroke::new(
                        1_f32,
                        if response.hovered() || response.has_focus() {
                            VIOLET
                        } else {
                            BORDER
                        },
                    ),
                );
                let image = Rect::from_min_size(rect.min + vec2(6., 6.), vec2(76., 64.));
                previews[i].paint(ui, image, colors[i]);
                p.text(
                    rect.left_center() + vec2(92., 0.),
                    Align2::LEFT_CENTER,
                    name,
                    bold(15.),
                    INK,
                );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name)
                });
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    choice = Some(i);
                }
            }
        });
    }
    choice
}
