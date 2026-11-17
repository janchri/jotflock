//! Structured story maps reuse frames and cards. Slots reference stable header IDs;
//! positions are derived, so column/release reordering survives document merges.
use crate::document::{Board, Card, Kind};
use anyhow::{anyhow, ensure, Result};
use automerge::{transaction::Transactable, AutoCommit, ObjType, ReadDoc, ROOT};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub column: String,
    pub lane: String,
}
#[derive(Clone)]
pub struct Axes {
    pub columns: Vec<String>,
    pub lanes: Vec<String>,
}
fn axes(doc: &AutoCommit, id: &str, cards: &[Card]) -> Option<Axes> {
    let (_, frame) = doc.get(ROOT, id).ok()??;
    let read = |key: &str| -> Option<Vec<String>> {
        let (_, list) = doc.get(&frame, key).ok()??;
        let mut seen = HashSet::new();
        Some(
            (0..doc.length(&list))
                .filter_map(|i| {
                    doc.get(&list, i)
                        .ok()
                        .flatten()
                        .and_then(|(v, _)| v.as_str().map(str::to_owned))
                })
                .filter(|id| {
                    seen.insert(id.clone())
                        && cards.iter().any(|c| c.id == *id && c.kind == Kind::Text)
                })
                .take(12)
                .collect(),
        )
    };
    Some(Axes {
        columns: read("columns")?,
        lanes: read("lanes")?,
    })
}
fn row_heights(cards: &[Card], frame: &str, axes: &Axes) -> Vec<f32> {
    axes.lanes
        .iter()
        .map(|lane| {
            let count = axes
                .columns
                .iter()
                .map(|column| {
                    cards
                        .iter()
                        .filter(|c| {
                            c.parent.as_deref() == Some(frame)
                                && c.story
                                    .as_ref()
                                    .is_some_and(|s| &s.lane == lane && &s.column == column)
                        })
                        .count()
                })
                .max()
                .unwrap_or(0);
            (count as f32 * 180. + 30.).clamp(220., 1500.)
        })
        .collect()
}
pub fn project(doc: &AutoCommit, cards: &mut [Card]) {
    let frames: Vec<_> = cards
        .iter()
        .filter(|c| c.kind == Kind::Frame)
        .filter_map(|f| axes(doc, &f.id, cards).map(|a| (f.clone(), a)))
        .collect();
    for (frame, axes) in frames {
        let heights = row_heights(cards, &frame.id, &axes);
        let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for c in cards
            .iter()
            .filter(|c| c.parent.as_ref() == Some(&frame.id))
        {
            if let Some(slot) = &c.story {
                groups
                    .entry((slot.column.clone(), slot.lane.clone()))
                    .or_default()
                    .push(c.id.clone());
            }
        }
        for values in groups.values_mut() {
            values.sort();
        }
        for c in cards.iter_mut() {
            if c.id == frame.id {
                c.width = c.width.max(200. + axes.columns.len() as f32 * 280.);
                c.height = c.height.max(170. + heights.iter().sum::<f32>());
            }
            if let Some(i) = axes.columns.iter().position(|id| id == &c.id) {
                c.x = frame.x + 180. + i as f32 * 280.;
                c.y = frame.y + 50.;
                c.width = 252.;
                c.height = 80.;
                c.parent = Some(frame.id.clone());
                c.layout_header = true;
            }
            if let Some(i) = axes.lanes.iter().position(|id| id == &c.id) {
                c.x = frame.x + 12.;
                c.y = frame.y + 150. + heights[..i].iter().sum::<f32>();
                c.width = 150.;
                c.height = 80.;
                c.parent = Some(frame.id.clone());
                c.layout_header = true;
            }
            if c.parent.as_ref() != Some(&frame.id) {
                continue;
            }
            if let Some(slot) = &c.story {
                if let (Some(col), Some(row)) = (
                    axes.columns.iter().position(|id| id == &slot.column),
                    axes.lanes.iter().position(|id| id == &slot.lane),
                ) {
                    let group = &groups[&(slot.column.clone(), slot.lane.clone())];
                    let i = group.iter().position(|id| id == &c.id).unwrap_or(0);
                    let step = 180_f32
                        .min((heights[row] - 180.) / (group.len().saturating_sub(1).max(1) as f32));
                    c.x = frame.x + 190. + col as f32 * 280.;
                    c.y = frame.y + 165. + heights[..row].iter().sum::<f32>() + i as f32 * step;
                    c.width = 232.;
                    c.height = 158.;
                }
            }
        }
    }
}
impl Board {
    pub fn story_axes(&self, frame: &str) -> Option<Axes> {
        axes(&self.doc, frame, &self.cards())
    }
    pub fn make_story_map(&mut self, frame: &str) -> Result<()> {
        ensure!(
            self.cards()
                .iter()
                .any(|c| c.id == frame && c.kind == Kind::Frame),
            "Select a frame"
        );
        let (_, obj) = self
            .doc
            .get(ROOT, frame)?
            .ok_or_else(|| anyhow!("Missing frame"))?;
        self.doc.put_object(&obj, "columns", ObjType::List)?;
        self.doc.put_object(obj, "lanes", ObjType::List)?;
        Ok(())
    }
    pub fn add_axis(&mut self, frame: &str, column: bool, title: &str) -> Result<String> {
        let axes = self
            .story_axes(frame)
            .ok_or_else(|| anyhow!("Not a story map"))?;
        let length = if column {
            axes.columns.len()
        } else {
            axes.lanes.len()
        };
        ensure!(
            length < 12,
            "A story map supports up to 12 activities and 12 releases"
        );
        let f = self
            .cards()
            .into_iter()
            .find(|c| c.id == frame)
            .ok_or_else(|| anyhow!("Missing frame"))?;
        let header = self.add(Kind::Text, title, f.x + 20., f.y + 50.)?;
        let (_, obj) = self
            .doc
            .get(ROOT, frame)?
            .ok_or_else(|| anyhow!("Missing frame"))?;
        let (_, list) = self
            .doc
            .get(obj, if column { "columns" } else { "lanes" })?
            .ok_or_else(|| anyhow!("Missing axis"))?;
        self.doc
            .insert(&list, self.doc.length(&list), header.as_str())?;
        Ok(header)
    }
    pub fn move_axis(
        &mut self,
        frame: &str,
        column: bool,
        index: usize,
        delta: isize,
    ) -> Result<()> {
        let axes = self
            .story_axes(frame)
            .ok_or_else(|| anyhow!("Not a story map"))?;
        let mut ids = if column { axes.columns } else { axes.lanes };
        ensure!(index < ids.len(), "Unknown axis");
        let next = (index as isize + delta).clamp(0, ids.len() as isize - 1) as usize;
        ids.swap(index, next);
        let (_, obj) = self
            .doc
            .get(ROOT, frame)?
            .ok_or_else(|| anyhow!("Missing frame"))?;
        let (_, list) = self
            .doc
            .get(obj, if column { "columns" } else { "lanes" })?
            .ok_or_else(|| anyhow!("Missing axis"))?;
        for i in (0..self.doc.length(&list)).rev() {
            self.doc.delete(&list, i)?;
        }
        for (i, id) in ids.iter().enumerate() {
            self.doc.insert(&list, i, id.as_str())?;
        }
        Ok(())
    }
    pub fn assign_slot(&mut self, id: &str, column: &str, lane: &str) -> Result<()> {
        let card = self
            .cards()
            .into_iter()
            .find(|c| c.id == id)
            .ok_or_else(|| anyhow!("Missing story"))?;
        let frame = card
            .parent
            .as_ref()
            .ok_or_else(|| anyhow!("Story must belong to a map"))?;
        let axes = self
            .story_axes(frame)
            .ok_or_else(|| anyhow!("Not a story map"))?;
        ensure!(
            axes.columns.iter().any(|s| s == column) && axes.lanes.iter().any(|s| s == lane),
            "Unknown activity or release"
        );
        let (_, obj) = self
            .doc
            .get(ROOT, id)?
            .ok_or_else(|| anyhow!("Missing story"))?;
        self.doc.put(&obj, "column", column)?;
        self.doc.put(obj, "lane", lane)?;
        Ok(())
    }
    pub fn place_story_drop(&mut self, id: &str) -> Result<()> {
        let cards = self.cards();
        let Some(c) = cards.iter().find(|c| c.id == id) else {
            return Ok(());
        };
        if c.layout_header {
            return Ok(());
        }
        let Some(frame) = c
            .parent
            .as_ref()
            .and_then(|id| cards.iter().find(|f| &f.id == id))
        else {
            return Ok(());
        };
        let Some(axes) = axes(&self.doc, &frame.id, &cards) else {
            return Ok(());
        };
        if axes.columns.is_empty() || axes.lanes.is_empty() {
            return Ok(());
        }
        let center = c.bounds().center();
        let col = ((center[0] - frame.x - 180.) / 280.).floor() as isize;
        if col < 0 || col >= axes.columns.len() as isize {
            return Ok(());
        }
        let mut y = frame.y + 150.;
        for (row, height) in row_heights(&cards, &frame.id, &axes)
            .into_iter()
            .enumerate()
        {
            if center[1] >= y && center[1] < y + height {
                return self.assign_slot(id, &axes.columns[col as usize], &axes.lanes[row]);
            }
            y += height;
        }
        Ok(())
    }
}
