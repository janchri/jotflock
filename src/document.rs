//! Transport-independent CRDT. Each object is an independent map and its label is shared text.
use crate::geometry::{Bounds, Shape};
use anyhow::{anyhow, ensure, Result};
use automerge::{transaction::Transactable, AutoCommit, ObjType, ReadDoc, ROOT};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub enum Kind {
    Sticky,
    Component,
    Text,
    Frame,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sticky => "Sticky note",
            Self::Component => "Component",
            Self::Text => "Text",
            Self::Frame => "Frame",
        }
    }
    fn from_code(v: &str) -> Self {
        match v {
            "Component" => Self::Component,
            "Text" => Self::Text,
            "Frame" => Self::Frame,
            _ => Self::Sticky,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub id: String,
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub kind: Kind,
    pub width: f32,
    pub height: f32,
    pub shape: Shape,
    pub parent: Option<String>,
    pub story: Option<crate::story_map::Slot>,
    pub layout_header: bool,
}
impl Default for Card {
    fn default() -> Self {
        Self {
            id: String::new(),
            text: String::new(),
            x: 0.,
            y: 0.,
            kind: Kind::Sticky,
            width: 232.,
            height: 158.,
            shape: Shape::Rectangle,
            parent: None,
            story: None,
            layout_header: false,
        }
    }
}
impl Card {
    pub fn bounds(&self) -> Bounds {
        Bounds {
            x: self.x,
            y: self.y,
            width: self.width,
            height: self.height,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Connector {
    pub id: String,
    pub from: String,
    pub to: String,
    pub text: String,
}
pub struct Board {
    pub doc: AutoCommit,
}
impl Board {
    pub fn new() -> Result<Self> {
        let mut doc = AutoCommit::new();
        doc.put(ROOT, "schema", 2_u64)?;
        doc.put(ROOT, "id", Uuid::new_v4().to_string())?;
        doc.put(ROOT, "title", "Untitled board")?;
        Ok(Self { doc })
    }
    pub fn load(bytes: &[u8]) -> Result<Self> {
        let doc = AutoCommit::load(bytes)?;
        let mut board = Self { doc };
        board.validate()?;
        // Schema 1 cards retain their original dimensions; old Frame cards become
        // actual containers without moving or implicitly adopting other objects.
        if board.string("schema") == "1" {
            board.doc.put(ROOT, "schema", 2_u64)?;
        }
        Ok(board)
    }
    /// Validate supported metadata and live object geometry before using a document.
    pub fn validate(&self) -> Result<()> {
        ensure!(
            ["1", "2"].contains(&self.string("schema").as_str())
                && Uuid::parse_str(&self.id()).is_ok(),
            "Unsupported or invalid board"
        );
        for card in self.cards() {
            Self::validate_geometry(&card)?;
            let (_, obj) = self
                .doc
                .get(ROOT, &card.id)?
                .ok_or_else(|| anyhow!("Missing object"))?;
            let (value, _) = self
                .doc
                .get(obj, "text")?
                .ok_or_else(|| anyhow!("Missing object text"))?;
            ensure!(
                matches!(value, automerge::Value::Object(ObjType::Text)),
                "Invalid object text"
            );
        }
        Ok(())
    }
    fn validate_geometry(card: &Card) -> Result<()> {
        ensure!(
            card.x.is_finite()
                && card.y.is_finite()
                && card.width.is_finite()
                && card.height.is_finite()
                && (120.0..=20_000.0).contains(&card.width)
                && (80.0..=20_000.0).contains(&card.height),
            "Invalid object geometry"
        );
        Ok(())
    }
    fn string(&self, key: &str) -> String {
        self.doc
            .get(ROOT, key)
            .ok()
            .flatten()
            .map(|(v, _)| {
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            })
            .unwrap_or_default()
    }
    pub fn id(&self) -> String {
        self.string("id")
    }
    pub fn title(&self) -> String {
        self.string("title")
    }
    pub fn rename(&mut self, title: &str) -> Result<()> {
        self.doc.put(ROOT, "title", title)?;
        Ok(())
    }
    pub fn save(&mut self) -> Vec<u8> {
        self.doc.save()
    }
    pub fn cards(&self) -> Vec<Card> {
        let mut cards: Vec<Card> = self
            .doc
            .keys(ROOT)
            .filter(|key| key.starts_with("card:"))
            .filter_map(|key| {
                let (_, obj) = self.doc.get(ROOT, &key).ok()??;
                let get = |p: &str| {
                    self.doc
                        .get(&obj, p)
                        .ok()
                        .flatten()
                        .map(|(v, _)| {
                            v.as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| v.to_string())
                        })
                        .unwrap_or_default()
                };
                if get("deleted") == "true" {
                    return None;
                }
                let text = self
                    .doc
                    .get(&obj, "text")
                    .ok()
                    .flatten()
                    .and_then(|(_, id)| self.doc.text(id).ok())
                    .unwrap_or_default();
                Some(Card {
                    id: key,
                    text,
                    x: get("x").parse().unwrap_or(0.),
                    y: get("y").parse().unwrap_or(0.),
                    kind: Kind::from_code(&get("kind")),
                    width: get("width").parse().unwrap_or(232.),
                    height: get("height").parse().unwrap_or(158.),
                    shape: Shape::from_name(&get("shape")),
                    story: {
                        let column = get("column");
                        let lane = get("lane");
                        if column.is_empty() || lane.is_empty() {
                            None
                        } else {
                            Some(crate::story_map::Slot { column, lane })
                        }
                    },
                    layout_header: false,
                    parent: {
                        let parent = get("parent");
                        if parent.is_empty() {
                            None
                        } else {
                            Some(parent)
                        }
                    },
                })
            })
            .collect();
        let frames: std::collections::HashSet<_> = cards
            .iter()
            .filter(|c| c.kind == Kind::Frame)
            .map(|c| c.id.clone())
            .collect();
        for card in &mut cards {
            if card.kind == Kind::Frame
                || card.parent.as_ref().is_some_and(|id| !frames.contains(id))
            {
                card.parent = None;
            }
        }
        crate::story_map::project(&self.doc, &mut cards);
        cards
    }
    pub fn add(&mut self, kind: Kind, text: &str, x: f32, y: f32) -> Result<String> {
        ensure!(
            x.is_finite() && y.is_finite() && text.len() <= 32_000,
            "Invalid object input"
        );
        let id = format!("card:{}", Uuid::new_v4());
        let obj = self.doc.put_object(ROOT, &id, ObjType::Map)?;
        self.doc.put(&obj, "kind", format!("{kind:?}"))?;
        self.doc.put(&obj, "x", x as f64)?;
        self.doc.put(&obj, "y", y as f64)?;
        self.doc.put(
            &obj,
            "width",
            if kind == Kind::Frame {
                720_f64
            } else {
                232_f64
            },
        )?;
        self.doc.put(
            &obj,
            "height",
            if kind == Kind::Frame {
                480_f64
            } else {
                158_f64
            },
        )?;
        self.doc.put(&obj, "shape", "Rectangle")?;
        let label = self.doc.put_object(&obj, "text", ObjType::Text)?;
        self.doc.splice_text(label, 0, 0, text)?;
        Ok(id)
    }
    pub fn edit(&mut self, card: &Card) -> Result<()> {
        Self::validate_geometry(card)?;
        ensure!(card.text.len() <= 32_000, "Object text is too long");
        let (_, obj) = self
            .doc
            .get(ROOT, &card.id)?
            .ok_or_else(|| anyhow!("Missing card"))?;
        let previous = self
            .cards()
            .into_iter()
            .find(|c| c.id == card.id)
            .ok_or_else(|| anyhow!("Missing live object"))?;
        let (value, text) = self
            .doc
            .get(&obj, "text")?
            .ok_or_else(|| anyhow!("Missing text"))?;
        ensure!(
            matches!(value, automerge::Value::Object(ObjType::Text)),
            "Invalid object text"
        );
        // Preflight all members before writing anything. Use wider intermediate
        // arithmetic so opposite finite extremes cannot overflow the translation.
        let mut translated = Vec::new();
        if previous.kind == Kind::Frame {
            let dx = card.x as f64 - previous.x as f64;
            let dy = card.y as f64 - previous.y as f64;
            if dx != 0. || dy != 0. {
                for mut child in self
                    .cards()
                    .into_iter()
                    .filter(|c| c.parent.as_ref() == Some(&card.id))
                {
                    child.x = (child.x as f64 + dx) as f32;
                    child.y = (child.y as f64 + dy) as f32;
                    Self::validate_geometry(&child)?;
                    let (_, child_obj) = self
                        .doc
                        .get(ROOT, &child.id)?
                        .ok_or_else(|| anyhow!("Missing child"))?;
                    translated.push((child_obj, child.x, child.y));
                }
            }
        }
        if previous.story.is_some() && (previous.x != card.x || previous.y != card.y) {
            self.doc.put(&obj, "column", "")?;
            self.doc.put(&obj, "lane", "")?;
        }
        // Only write changed fields; text edits do not generate position operations.
        for (key, value) in [
            ("x", card.x as f64),
            ("y", card.y as f64),
            ("width", card.width as f64),
            ("height", card.height as f64),
        ] {
            let old = self.doc.get(&obj, key)?.and_then(|(v, _)| v.as_f64());
            if old != Some(value) {
                self.doc.put(&obj, key, value)?;
            }
        }
        self.doc.update_text(&text, &card.text)?;
        for (child, x, y) in translated {
            self.doc.put(&child, "x", x as f64)?;
            self.doc.put(&child, "y", y as f64)?;
        }
        Ok(())
    }
    pub fn reparent(&mut self, id: &str) -> Result<()> {
        let cards = self.cards();
        let card = cards
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("Missing object"))?;
        if card.kind == Kind::Frame {
            return Ok(());
        }
        let parent = cards
            .iter()
            .filter(|c| c.kind == Kind::Frame)
            .filter(|frame| {
                let area = Bounds {
                    y: frame.y + 36.,
                    height: frame.height - 36.,
                    ..frame.bounds()
                };
                area.contains(card.bounds())
            })
            .min_by(|a, b| {
                (a.width * a.height)
                    .total_cmp(&(b.width * b.height))
                    .then(a.id.cmp(&b.id))
            })
            .map(|c| c.id.as_str())
            .unwrap_or("");
        let (_, obj) = self
            .doc
            .get(ROOT, id)?
            .ok_or_else(|| anyhow!("Missing object"))?;
        self.doc.put(&obj, "parent", parent)?;
        self.doc.put(&obj, "column", "")?;
        self.doc.put(obj, "lane", "")?;
        self.place_story_drop(id)?;
        Ok(())
    }
    pub fn set_shape(&mut self, id: &str, shape: Shape) -> Result<()> {
        let card = self
            .cards()
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("Missing object"))?;
        if card.kind != Kind::Component {
            return Err(anyhow!("Only components have diagram shapes"));
        }
        let (_, obj) = self
            .doc
            .get(ROOT, id)?
            .ok_or_else(|| anyhow!("Missing object"))?;
        self.doc.put(obj, "shape", shape.name())?;
        Ok(())
    }
    pub fn connectors(&self) -> Vec<Connector> {
        let endpoints: std::collections::HashSet<_> = self
            .cards()
            .into_iter()
            .filter(|c| c.kind != Kind::Frame)
            .map(|c| c.id)
            .collect();
        self.doc
            .keys(ROOT)
            .filter(|key| key.starts_with("edge:"))
            .filter_map(|id| {
                let (_, obj) = self.doc.get(ROOT, &id).ok()??;
                let get = |key| {
                    self.doc
                        .get(&obj, key)
                        .ok()
                        .flatten()
                        .and_then(|(v, _)| v.as_str().map(str::to_owned))
                        .unwrap_or_default()
                };
                if self
                    .doc
                    .get(&obj, "deleted")
                    .ok()
                    .flatten()
                    .is_some_and(|(v, _)| v.as_bool() == Some(true))
                {
                    return None;
                }
                let (from, to) = (get("from"), get("to"));
                if from == to || !endpoints.contains(&from) || !endpoints.contains(&to) {
                    return None;
                }
                let text = self
                    .doc
                    .get(&obj, "text")
                    .ok()
                    .flatten()
                    .and_then(|(_, id)| self.doc.text(id).ok())
                    .unwrap_or_default();
                Some(Connector { id, from, to, text })
            })
            .collect()
    }
    pub fn connect(&mut self, from: &str, to: &str) -> Result<String> {
        let cards = self.cards();
        if from == to
            || [from, to]
                .iter()
                .any(|id| !cards.iter().any(|c| c.id == *id && c.kind != Kind::Frame))
        {
            return Err(anyhow!("Connect two different non-frame objects"));
        }
        let id = format!("edge:{}", Uuid::new_v4());
        let obj = self.doc.put_object(ROOT, &id, ObjType::Map)?;
        self.doc.put(&obj, "from", from)?;
        self.doc.put(&obj, "to", to)?;
        self.doc.put_object(&obj, "text", ObjType::Text)?;
        Ok(id)
    }
    pub fn label_connector(&mut self, id: &str, label: &str) -> Result<()> {
        ensure!(
            id.starts_with("edge:") && self.connectors().iter().any(|edge| edge.id == id),
            "Missing live connector"
        );
        if label.len() > 32_000 {
            return Err(anyhow!("Label is too long"));
        }
        let (_, obj) = self
            .doc
            .get(ROOT, id)?
            .ok_or_else(|| anyhow!("Missing connector"))?;
        let (value, text) = self
            .doc
            .get(obj, "text")?
            .ok_or_else(|| anyhow!("Missing label"))?;
        ensure!(
            matches!(value, automerge::Value::Object(ObjType::Text)),
            "Invalid connector label"
        );
        self.doc.update_text(&text, label)?;
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> Result<()> {
        let live = self.cards();
        ensure!(
            !live.iter().any(|c| c.id == id && c.layout_header),
            "Rename or reorder story-map headers through the map controls"
        );
        ensure!(
            id.starts_with("card:") || id.starts_with("edge:"),
            "Invalid object ID"
        );
        let (value, obj) = self
            .doc
            .get(ROOT, id)?
            .ok_or_else(|| anyhow!("Missing object"))?;
        ensure!(
            matches!(value, automerge::Value::Object(ObjType::Map)),
            "Invalid object record"
        );
        if live.iter().any(|c| c.id == id && c.kind == Kind::Frame) {
            for child in live.iter().filter(|c| c.parent.as_deref() == Some(id)) {
                let (_, child_obj) = self
                    .doc
                    .get(ROOT, &child.id)?
                    .ok_or_else(|| anyhow!("Missing child"))?;
                for (key, value) in [
                    ("x", child.x),
                    ("y", child.y),
                    ("width", child.width),
                    ("height", child.height),
                ] {
                    self.doc.put(&child_obj, key, value as f64)?;
                }
                for key in ["parent", "column", "lane"] {
                    self.doc.put(&child_obj, key, "")?;
                }
            }
        }
        // Edges with deleted endpoints and children with deleted parents are
        // resolved by the read model, including when edits merge concurrently.
        self.doc.put(obj, "deleted", true)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn offline_text_and_move_merge_and_reload() -> Result<()> {
        let mut a = Board::new()?;
        a.add(Kind::Sticky, "Hello", 0., 0.)?;
        let mut b = Board::load(&a.save())?;
        let mut ca = a.cards().remove(0);
        ca.text = "Hello team".into();
        a.edit(&ca)?;
        let mut cb = b.cards().remove(0);
        cb.x = 120.;
        b.edit(&cb)?;
        a.doc.merge(&mut b.doc)?;
        b.doc.merge(&mut a.doc)?;
        let card = a.cards().remove(0);
        assert_eq!(card.text, "Hello team");
        assert_eq!(card.x, 120.);
        assert_eq!(b.cards()[0].text, card.text);
        let c = Board::load(&a.save())?;
        assert_eq!(c.cards()[0].x, 120.);
        Ok(())
    }
    #[test]
    fn reliable_stream_sync_converges_after_partition() -> Result<()> {
        use automerge::sync::{Message, State, SyncDoc};
        fn sync(a: &mut Board, b: &mut Board) -> Result<()> {
            let (mut sa, mut sb) = (State::new(), State::new());
            for _ in 0..100 {
                let ab = a.doc.sync().generate_sync_message(&mut sa);
                let ba = b.doc.sync().generate_sync_message(&mut sb);
                if ab.is_none() && ba.is_none() {
                    return Ok(());
                }
                if let Some(msg) = ab {
                    b.doc
                        .sync()
                        .receive_sync_message(&mut sb, Message::decode(&msg.encode())?)?;
                }
                if let Some(msg) = ba {
                    a.doc
                        .sync()
                        .receive_sync_message(&mut sa, Message::decode(&msg.encode())?)?;
                }
            }
            Err(anyhow!("Sync did not converge"))
        }
        let mut a = Board::new()?;
        a.add(Kind::Sticky, "Seed", 0., 0.)?;
        let bytes = a.save();
        let mut b = Board::load(&bytes)?;
        let mut c = Board::load(&bytes)?;
        a.add(Kind::Component, "A", 1., 1.)?;
        b.add(Kind::Component, "B", 2., 2.)?;
        c.add(Kind::Component, "C", 3., 3.)?;
        sync(&mut a, &mut b)?;
        sync(&mut b, &mut c)?;
        sync(&mut c, &mut a)?;
        sync(&mut a, &mut b)?;
        for board in [&a, &b, &c] {
            assert_eq!(board.cards().len(), 4);
        }
        assert_eq!(a.doc.get_heads(), b.doc.get_heads());
        assert_eq!(b.doc.get_heads(), c.doc.get_heads());
        Ok(())
    }
    #[test]
    fn delete_wins_over_concurrent_edit() -> Result<()> {
        let mut a = Board::new()?;
        a.add(Kind::Sticky, "Hello", 0., 0.)?;
        let mut b = Board::load(&a.save())?;
        let mut card = b.cards().remove(0);
        a.delete(&card.id)?;
        card.text = "Edited offline".into();
        b.edit(&card)?;
        a.doc.merge(&mut b.doc)?;
        b.doc.merge(&mut a.doc)?;
        assert!(a.cards().is_empty());
        assert!(b.cards().is_empty());
        Ok(())
    }
    #[test]
    fn frames_adopt_move_release_and_preserve_children_on_delete() -> Result<()> {
        let mut board = Board::new()?;
        let frame = board.add(Kind::Frame, "Ideas", 0., 0.)?;
        let child = board.add(Kind::Sticky, "Member", 50., 80.)?;
        board.reparent(&child)?;
        assert_eq!(
            board
                .cards()
                .iter()
                .find(|c| c.id == child)
                .unwrap()
                .parent
                .as_ref(),
            Some(&frame)
        );
        let mut board = Board::load(&board.save())?;
        let mut f = board.cards().into_iter().find(|c| c.id == frame).unwrap();
        f.x = 100.;
        f.y = 40.;
        board.edit(&f)?;
        let mut c = board.cards().into_iter().find(|c| c.id == child).unwrap();
        assert_eq!((c.x, c.y), (150., 120.));
        f.width = 900.;
        board.edit(&f)?;
        assert_eq!(
            board.cards().iter().find(|c| c.id == child).unwrap().x,
            150.
        );
        c.x = 1500.;
        board.edit(&c)?;
        board.reparent(&child)?;
        assert!(board
            .cards()
            .iter()
            .find(|c| c.id == child)
            .unwrap()
            .parent
            .is_none());
        c.x = 150.;
        board.edit(&c)?;
        board.reparent(&child)?;
        board.delete(&frame)?;
        let c = board.cards().remove(0);
        assert_eq!(c.id, child);
        assert!(c.parent.is_none());
        Ok(())
    }
    #[test]
    fn membership_selects_smallest_containing_frame_and_does_not_infer_nested_frames() -> Result<()>
    {
        let mut board = Board::new()?;
        let large = board.add(Kind::Frame, "Large", 0., 0.)?;
        let small = board.add(Kind::Frame, "Small", 30., 40.)?;
        let mut f = board.cards().into_iter().find(|c| c.id == small).unwrap();
        f.width = 400.;
        f.height = 300.;
        board.edit(&f)?;
        let child = board.add(Kind::Sticky, "Inside both", 60., 100.)?;
        board.reparent(&child)?;
        board.reparent(&small)?;
        assert_eq!(
            board
                .cards()
                .iter()
                .find(|c| c.id == child)
                .unwrap()
                .parent
                .as_ref(),
            Some(&small)
        );
        assert!(board
            .cards()
            .iter()
            .find(|c| c.id == small)
            .unwrap()
            .parent
            .is_none());
        board.delete(&large)?;
        assert_eq!(
            board
                .cards()
                .iter()
                .find(|c| c.id == child)
                .unwrap()
                .parent
                .as_ref(),
            Some(&small)
        );
        Ok(())
    }
    #[test]
    fn connectors_and_shapes_survive_reload_and_hide_deleted_endpoints() -> Result<()> {
        let mut board = Board::new()?;
        let a = board.add(Kind::Component, "API", 0., 0.)?;
        let b = board.add(Kind::Component, "Database", 500., 0.)?;
        board.set_shape(&b, Shape::Ellipse)?;
        let edge = board.connect(&a, &b)?;
        board.label_connector(&edge, "queries")?;
        let mut board = Board::load(&board.save())?;
        assert_eq!(board.connectors()[0].text, "queries");
        assert_eq!(board.connectors()[0].from, a);
        assert_eq!(
            board.cards().iter().find(|c| c.id == b).unwrap().shape,
            Shape::Ellipse
        );
        assert!(board.connect(&a, &a).is_err());
        let frame = board.add(Kind::Frame, "Area", 0., 0.)?;
        assert!(board.connect(&a, &frame).is_err());
        board.delete(&b)?;
        assert!(board.connectors().is_empty());
        Ok(())
    }
    #[test]
    fn concurrent_parent_and_endpoint_deletion_resolve_after_merge() -> Result<()> {
        let mut a = Board::new()?;
        let parent = a.add(Kind::Frame, "Area", 0., 0.)?;
        let child = a.add(Kind::Sticky, "Child", 50., 80.)?;
        let target = a.add(Kind::Component, "Target", 800., 0.)?;
        let mut b = Board::load(&a.save())?;
        a.delete(&parent)?;
        a.delete(&target)?;
        b.reparent(&child)?;
        b.connect(&child, &target)?;
        a.doc.merge(&mut b.doc)?;
        b.doc.merge(&mut a.doc)?;
        for board in [&a, &b] {
            assert!(board.connectors().is_empty());
            assert!(board
                .cards()
                .iter()
                .find(|c| c.id == child)
                .unwrap()
                .parent
                .is_none());
        }
        Ok(())
    }
    #[test]
    fn schema_one_load_keeps_objects_positions_and_text() -> Result<()> {
        let mut old = AutoCommit::new();
        old.put(ROOT, "schema", 1_u64)?;
        old.put(ROOT, "id", Uuid::new_v4().to_string())?;
        old.put(ROOT, "title", "Older board")?;
        let obj = old.put_object(ROOT, "card:legacy", ObjType::Map)?;
        old.put(&obj, "kind", "Frame")?;
        old.put(&obj, "x", 10_f64)?;
        old.put(&obj, "y", 20_f64)?;
        let text = old.put_object(&obj, "text", ObjType::Text)?;
        old.splice_text(text, 0, 0, "Keep this")?;
        let board = Board::load(&old.save())?;
        assert_eq!(board.string("schema"), "2");
        let card = board.cards().remove(0);
        assert_eq!(
            (card.x, card.y, card.width, card.height),
            (10., 20., 232., 158.)
        );
        assert_eq!(card.text, "Keep this");
        assert!(card.parent.is_none());
        Ok(())
    }
}
