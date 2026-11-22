//! Templates compose the same document objects as manual editing; insertion is atomic.
use crate::{
    document::{Board, Kind},
    geometry::Shape,
};
use anyhow::Result;
use automerge::{transaction::Transactable, ReadDoc, ROOT};
pub const NAMES: [&str; 3] = ["Brainstorming", "Architecture", "User story map"];

fn sized(
    board: &mut Board,
    kind: Kind,
    label: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) -> Result<String> {
    let id = board.add(kind, label, x, y)?;
    let mut c = board.cards().into_iter().find(|c| c.id == id).unwrap();
    c.width = w;
    c.height = h;
    board.edit(&c)?;
    Ok(id)
}
fn child(board: &mut Board, frame: &str, label: &str, x: f32, y: f32) -> Result<String> {
    let id = board.add(Kind::Sticky, label, x, y)?;
    let (_, obj) = board.doc.get(ROOT, &id)?.unwrap();
    board.doc.put(obj, "parent", frame)?;
    Ok(id)
}
pub fn insert(board: &mut Board, template: usize) -> Result<String> {
    let mut candidate = Board {
        doc: board.doc.clone(),
    };
    let x = board
        .cards()
        .iter()
        .map(|c| c.x + c.width + 120.)
        .fold(40_f32, f32::max);
    let y = 40.;
    let selected = match template {
        1 => {
            let f = sized(
                &mut candidate,
                Kind::Frame,
                "System architecture",
                x,
                y,
                1460.,
                850.,
            )?;
            let mut ids = Vec::new();
            for (label, dx, dy, shape) in [
                ("Web application", 60., 110., Shape::Rectangle),
                ("API service", 430., 110., Shape::Rectangle),
                ("Database", 1000., 110., Shape::Ellipse),
                ("Authentication", 60., 450., Shape::Diamond),
                ("Background worker", 430., 450., Shape::Rectangle),
                ("Message queue", 1000., 450., Shape::Rectangle),
            ] {
                let id = candidate.add(Kind::Component, label, x + dx, y + dy)?;
                candidate.set_shape(&id, shape)?;
                candidate.reparent(&id)?;
                ids.push(id);
            }
            for (a, b, label) in [
                (0, 1, "HTTPS"),
                (1, 2, "Read / write"),
                (1, 3, "Validate identity"),
                (1, 5, "Publish work"),
                (5, 4, "Consume work"),
                (4, 2, "Update results"),
            ] {
                let e = candidate.connect(&ids[a], &ids[b])?;
                candidate.label_connector(&e, label)?;
            }
            f
        }
        2 => {
            let f = sized(
                &mut candidate,
                Kind::Frame,
                "User story map",
                x,
                y,
                1060.,
                850.,
            )?;
            candidate.make_story_map(&f)?;
            let columns = ["Discover", "Decide", "Order"]
                .into_iter()
                .map(|t| candidate.add_axis(&f, true, t))
                .collect::<Result<Vec<_>>>()?;
            let lanes = ["MVP", "Next release", "Later"]
                .into_iter()
                .map(|t| candidate.add_axis(&f, false, t))
                .collect::<Result<Vec<_>>>()?;
            for (col, lane, label) in [
                (0, 0, "As a visitor, I can search products"),
                (1, 0, "As a buyer, I can compare options"),
                (2, 0, "As a buyer, I can complete checkout"),
                (0, 1, "As a visitor, I can save searches"),
                (1, 1, "As a buyer, I can read reviews"),
                (2, 2, "As a buyer, I can reorder"),
            ] {
                let id = child(&mut candidate, &f, label, x + 190., y + 165.)?;
                candidate.assign_slot(&id, &columns[col], &lanes[lane])?;
            }
            f
        }
        _ => {
            let mut first = String::new();
            for (i, title, prompts) in [
                (
                    0,
                    "1 · Define the challenge",
                    [
                        "What problem are we solving?",
                        "Who experiences it?",
                        "What would success look like?",
                    ],
                ),
                (
                    1,
                    "2 · Generate ideas",
                    [
                        "One idea per note",
                        "What if constraints disappeared?",
                        "Build on someone else's idea",
                    ],
                ),
                (
                    2,
                    "3 · Find themes",
                    [
                        "Move related ideas into this area",
                        "Name the emerging themes",
                        "Which assumptions need evidence?",
                    ],
                ),
                (
                    3,
                    "4 · Choose next steps",
                    [
                        "Choose an experiment",
                        "Assign an owner",
                        "Define a measurable outcome",
                    ],
                ),
            ] {
                let fx = x + (i % 2) as f32 * 820.;
                let fy = y + (i / 2) as f32 * 540.;
                let f = sized(&mut candidate, Kind::Frame, title, fx, fy, 780., 500.)?;
                if i == 0 {
                    first = f.clone();
                }
                for (j, prompt) in prompts.iter().enumerate() {
                    child(
                        &mut candidate,
                        &f,
                        prompt,
                        fx + 30. + (j % 3) as f32 * 245.,
                        fy + 90.,
                    )?;
                }
            }
            first
        }
    };
    candidate.validate()?;
    *board = candidate;
    Ok(selected)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axis_reordering_handles_extreme_offsets_and_skips_boundary_noops() -> Result<()> {
        let mut board = Board::new()?;
        let map = insert(&mut board, 2)?;
        let original = board.story_axes(&map).unwrap().columns;
        let heads = board.doc.get_heads();
        board.move_axis(&map, true, 0, isize::MIN)?;
        assert_eq!(board.doc.get_heads(), heads);
        board.move_axis(&map, true, 0, isize::MAX)?;
        let reordered = board.story_axes(&map).unwrap().columns;
        assert_eq!(reordered.last(), original.first());
        let heads = board.doc.get_heads();
        board.move_axis(&map, true, reordered.len() - 1, 1)?;
        assert_eq!(board.doc.get_heads(), heads);
        Ok(())
    }
    #[test]
    fn story_projection_cannot_hide_invalid_saved_geometry() -> Result<()> {
        let mut board = Board::new()?;
        let map = insert(&mut board, 2)?;
        let header = board.story_axes(&map).unwrap().columns[0].clone();
        let (_, obj) = board.doc.get(ROOT, &header)?.unwrap();
        board.doc.put(&obj, "width", f64::NAN)?;
        assert!(board.validate().is_err());
        assert!(Board::load(&board.save()).is_err());
        board.doc.put(&obj, "width", "invalid")?;
        assert!(board.validate().is_err());
        assert!(Board::load(&board.save()).is_err());
        Ok(())
    }
    #[test]
    fn architecture_has_attached_labeled_relationships() -> Result<()> {
        let mut b = Board::new()?;
        insert(&mut b, 1)?;
        assert_eq!(b.connectors().len(), 6);
        assert!(b.connectors().iter().all(|e| !e.text.is_empty()));
        assert_eq!(
            b.cards().iter().filter(|c| c.kind == Kind::Frame).count(),
            1
        );
        Ok(())
    }
    #[test]
    fn story_slots_survive_column_and_release_reordering_and_reload() -> Result<()> {
        let mut b = Board::new()?;
        let f = insert(&mut b, 2)?;
        let axes = b.story_axes(&f).unwrap();
        let before = b
            .cards()
            .into_iter()
            .find(|c| {
                c.story
                    .as_ref()
                    .is_some_and(|s| s.column == axes.columns[0] && s.lane == axes.lanes[0])
            })
            .unwrap();
        b.move_axis(&f, true, 0, 1)?;
        b.move_axis(&f, false, 0, 1)?;
        let mut b = Board::load(&b.save())?;
        let after = b.cards().into_iter().find(|c| c.id == before.id).unwrap();
        assert_eq!(before.story, after.story);
        assert!(before.x != after.x || before.y != after.y);
        let old = b.cards().len();
        insert(&mut b, 0)?;
        assert!(b.cards().len() > old);
        Ok(())
    }
}

#[cfg(test)]
mod consistency_tests {
    use super::*;
    #[test]
    fn story_slots_and_geometry_converge_after_concurrent_axis_changes() -> Result<()> {
        let mut a = Board::new()?;
        let map = insert(&mut a, 2)?;
        let slots: std::collections::BTreeMap<_, _> = a
            .cards()
            .into_iter()
            .filter_map(|c| c.story.map(|s| (c.id, s)))
            .collect();
        let mut b = Board::load(&a.save())?;
        a.move_axis(&map, true, 0, 1)?;
        b.move_axis(&map, false, 1, 1)?;
        b.add_axis(&map, true, "New activity")?;
        a.doc.merge(&mut b.doc)?;
        b.doc.merge(&mut a.doc)?;
        assert_eq!(a.cards(), b.cards());
        for c in a.cards() {
            if let Some(slot) = slots.get(&c.id) {
                assert_eq!(c.story.as_ref(), Some(slot));
            }
        }
        Ok(())
    }
    #[test]
    fn deleting_story_map_preserves_displayed_stories_and_positions() -> Result<()> {
        let mut board = Board::new()?;
        let map = insert(&mut board, 2)?;
        let before = board.cards();
        board.delete(&map)?;
        for card in board.cards() {
            let old = before.iter().find(|c| c.id == card.id).unwrap();
            assert_eq!((card.x, card.y), (old.x, old.y));
            assert!(card.parent.is_none());
            assert!(card.story.is_none());
        }
        Ok(())
    }
}
