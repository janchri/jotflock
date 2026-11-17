//! World-space geometry, independent of GUI and persistence.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum Shape {
    #[default]
    Rectangle,
    Ellipse,
    Diamond,
}
impl Shape {
    pub const ALL: [Self; 3] = [Self::Rectangle, Self::Ellipse, Self::Diamond];
    pub fn name(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Diamond => "Diamond",
        }
    }
    pub fn from_name(value: &str) -> Self {
        match value {
            "Ellipse" => Self::Ellipse,
            "Diamond" => Self::Diamond,
            _ => Self::Rectangle,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
impl Bounds {
    pub fn center(self) -> [f32; 2] {
        [self.x + self.width / 2., self.y + self.height / 2.]
    }
    pub fn contains(self, other: Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }
    /// Intersection of a center ray with the object's outline. Coincident centers
    /// choose the right edge deterministically rather than producing NaN.
    pub fn anchor(self, target: [f32; 2], shape: Shape) -> [f32; 2] {
        // World coordinates remain f32; f64 intermediates prevent subtraction
        // and squared distances from overflowing for finite document positions.
        let center = [
            self.x as f64 + self.width as f64 / 2.,
            self.y as f64 + self.height as f64 / 2.,
        ];
        let mut dx = target[0] as f64 - center[0];
        let dy = target[1] as f64 - center[1];
        if dx == 0. && dy == 0. {
            dx = 1.;
        }
        let rx = self.width as f64 / 2.;
        let ry = self.height as f64 / 2.;
        let factor = match shape {
            Shape::Rectangle => 1. / (dx.abs() / rx).max(dy.abs() / ry),
            Shape::Ellipse => 1. / ((dx / rx).powi(2) + (dy / ry).powi(2)).sqrt(),
            Shape::Diamond => 1. / (dx.abs() / rx + dy.abs() / ry),
        };
        [
            (center[0] + dx * factor) as f32,
            (center[1] + dy * factor) as f32,
        ]
    }
}
pub fn segment_distance(point: [f32; 2], start: [f32; 2], end: [f32; 2]) -> f32 {
    let (point, start, end) = (
        point.map(f64::from),
        start.map(f64::from),
        end.map(f64::from),
    );
    let d = [end[0] - start[0], end[1] - start[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0. {
        (((point[0] - start[0]) * d[0] + (point[1] - start[1]) * d[1]) / length).clamp(0., 1.)
    } else {
        0.
    };
    ((point[0] - start[0] - t * d[0]).powi(2) + (point[1] - start[1] - t * d[1]).powi(2)).sqrt()
        as f32
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn anchors_follow_shape_outline_and_remain_finite() {
        let b = Bounds {
            x: 0.,
            y: 0.,
            width: 200.,
            height: 100.,
        };
        for shape in Shape::ALL {
            assert_eq!(b.anchor([300., 50.], shape), [200., 50.]);
            assert_eq!(b.anchor(b.center(), shape), [200., 50.]);
            let p = b.anchor([300., 150.], shape);
            let nx = (p[0] - 100.) / 100.;
            let ny = (p[1] - 50.) / 50.;
            let outline = match shape {
                Shape::Rectangle => nx.abs().max(ny.abs()),
                Shape::Ellipse => nx * nx + ny * ny,
                Shape::Diamond => nx.abs() + ny.abs(),
            };
            assert!((outline - 1.).abs() < 0.0001);
        }
        assert_eq!(segment_distance([5., 3.], [0., 0.], [10., 0.]), 3.);
        assert_eq!(segment_distance([5., 0.], [0., 0.], [0., 0.]), 5.);
    }
}
