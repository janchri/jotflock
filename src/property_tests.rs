//! Generated tests of core invariants. These execute examples; they are not proofs.
use crate::{
    document::{Board, Card, Connector, Kind},
    geometry::Shape,
    workspace::Workspace,
};
use anyhow::{ensure, Result};
use automerge::ActorId;
use proptest::{prelude::*, test_runner::Config};
use std::collections::{BTreeMap, BTreeSet};

fn config() -> Config {
    let mut config = Config::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 64;
    }
    config
}
fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![Just('é'), Just('中'), Just('🙂'), any::<char>()],
        0..20,
    )
    .prop_map(|chars| chars.into_iter().collect())
}
#[derive(Clone, Debug)]
enum Op {
    Add(u8, i16, i16, String),
    Move(u8, i16, i16),
    Resize(u8, u16, u16),
    Text(u8, String),
    Delete(u8),
    Reparent(u8),
    Connect(u8, u8),
    Label(u8, String),
    DeleteEdge(u8),
    Shape(u8, u8),
    Reload,
}
fn operation() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0_u8..4, -1000_i16..1000, -1000_i16..1000, text()).prop_map(|(k,x,y,t)| Op::Add(k,x,y,t)),
        3 => (any::<u8>(), -500_i16..500, -500_i16..500).prop_map(|(i,x,y)| Op::Move(i,x,y)),
        2 => (any::<u8>(), 120_u16..2000, 80_u16..1500).prop_map(|(i,w,h)| Op::Resize(i,w,h)),
        2 => (any::<u8>(), text()).prop_map(|(i,t)| Op::Text(i,t)),
        2 => any::<u8>().prop_map(Op::Delete),
        2 => any::<u8>().prop_map(Op::Reparent),
        3 => (any::<u8>(),any::<u8>()).prop_map(|(a,b)| Op::Connect(a,b)),
        1 => (any::<u8>(),text()).prop_map(|(i,t)| Op::Label(i,t)),
        1 => any::<u8>().prop_map(Op::DeleteEdge),
        1 => (any::<u8>(),0_u8..3).prop_map(|(i,s)| Op::Shape(i,s)),
        2 => Just(Op::Reload),
    ]
}
#[derive(Clone, Default)]
struct Model {
    created: BTreeSet<String>,
    deleted: BTreeSet<String>,
    edges: BTreeMap<String, (String, String)>,
    deleted_edges: BTreeSet<String>,
}
fn kind(index: u8) -> Kind {
    [Kind::Sticky, Kind::Component, Kind::Text, Kind::Frame][index as usize % 4]
}
fn seed() -> Result<(Board, Model)> {
    let mut board = Board::new()?;
    board.doc.set_actor(ActorId::from(&[1][..]));
    let mut model = Model::default();
    for (kind, x, y) in [
        (Kind::Frame, 0., 0.),
        (Kind::Sticky, 50., 80.),
        (Kind::Component, 850., 80.),
    ] {
        model.created.insert(board.add(kind, "Seed", x, y)?);
    }
    Ok((board, model))
}
fn snapshot(board: &Board) -> (String, String, Vec<Card>, Vec<Connector>) {
    (board.id(), board.title(), board.cards(), board.connectors())
}
fn check(board: &Board, model: &Model) -> Result<()> {
    let cards = board.cards();
    let ids: BTreeSet<_> = cards.iter().map(|c| c.id.clone()).collect();
    let expected: BTreeSet<_> = model.created.difference(&model.deleted).cloned().collect();
    ensure!(ids == expected, "Objects lost or resurrected");
    for c in &cards {
        ensure!(
            c.x.is_finite() && c.y.is_finite() && c.width.is_finite() && c.height.is_finite(),
            "Non-finite geometry"
        );
        ensure!(
            (120.0..=20_000.0).contains(&c.width) && (80.0..=20_000.0).contains(&c.height),
            "Invalid dimensions"
        );
        if let Some(parent) = &c.parent {
            ensure!(
                c.kind != Kind::Frame
                    && cards
                        .iter()
                        .any(|p| &p.id == parent && p.kind == Kind::Frame),
                "Invalid or cyclic membership"
            );
        }
    }
    let edges = board.connectors();
    let expected_edges: BTreeSet<_> = model
        .edges
        .iter()
        .filter(|(id, (a, b))| {
            !model.deleted_edges.contains(*id) && ids.contains(a) && ids.contains(b)
        })
        .map(|(id, _)| id.clone())
        .collect();
    ensure!(
        edges.iter().map(|e| e.id.clone()).collect::<BTreeSet<_>>() == expected_edges,
        "Lost or dangling visible edge"
    );
    for e in edges {
        ensure!(
            e.from != e.to
                && [&e.from, &e.to]
                    .iter()
                    .all(|id| cards.iter().any(|c| &c.id == *id && c.kind != Kind::Frame)),
            "Invalid endpoint"
        );
    }
    Ok(())
}
fn apply(board: &mut Board, model: &mut Model, op: &Op) -> Result<()> {
    let cards = board.cards();
    let edges = board.connectors();
    let card = |i: u8| {
        if cards.is_empty() {
            None
        } else {
            Some(cards[i as usize % cards.len()].clone())
        }
    };
    let edge = |i: u8| {
        if edges.is_empty() {
            None
        } else {
            Some(edges[i as usize % edges.len()].clone())
        }
    };
    match op {
        Op::Add(k, x, y, t) => {
            model
                .created
                .insert(board.add(kind(*k), t, *x as f32, *y as f32)?);
        }
        Op::Move(i, dx, dy) => {
            if let Some(mut c) = card(*i) {
                c.x += *dx as f32;
                c.y += *dy as f32;
                board.edit(&c)?;
                board.reparent(&c.id)?;
            }
        }
        Op::Resize(i, w, h) => {
            if let Some(mut c) = card(*i) {
                c.width = *w as f32;
                c.height = *h as f32;
                board.edit(&c)?;
                board.reparent(&c.id)?;
            }
        }
        Op::Text(i, t) => {
            if let Some(mut c) = card(*i) {
                c.text = t.clone();
                board.edit(&c)?;
            }
        }
        Op::Delete(i) => {
            if let Some(c) = card(*i) {
                board.delete(&c.id)?;
                model.deleted.insert(c.id);
            }
        }
        Op::Reparent(i) => {
            if let Some(c) = card(*i) {
                board.reparent(&c.id)?;
            }
        }
        Op::Connect(i, j) => {
            if let (Some(a), Some(b)) = (card(*i), card(*j)) {
                let result = board.connect(&a.id, &b.id);
                if a.id == b.id || a.kind == Kind::Frame || b.kind == Kind::Frame {
                    ensure!(result.is_err(), "Invalid connection accepted");
                } else {
                    model.edges.insert(result?, (a.id, b.id));
                }
            }
        }
        Op::Label(i, t) => {
            if let Some(e) = edge(*i) {
                board.label_connector(&e.id, t)?;
            }
        }
        Op::DeleteEdge(i) => {
            if let Some(e) = edge(*i) {
                board.delete(&e.id)?;
                model.deleted_edges.insert(e.id);
            }
        }
        Op::Shape(i, s) => {
            if let Some(c) = card(*i) {
                if c.kind == Kind::Component {
                    board.set_shape(&c.id, Shape::ALL[*s as usize])?;
                }
            }
        }
        Op::Reload => {
            let before = snapshot(board);
            *board = Board::load(&board.save())?;
            ensure!(before == snapshot(board), "Save/load changed content");
        }
    }
    check(board, model)
}
fn union(models: &[Model]) -> Model {
    let mut result = Model::default();
    for model in models {
        result.created.extend(model.created.clone());
        result.deleted.extend(model.deleted.clone());
        result.edges.extend(model.edges.clone());
        result.deleted_edges.extend(model.deleted_edges.clone());
    }
    result
}
fn sync(a: &mut Board, b: &mut Board) -> Result<()> {
    use automerge::sync::{Message, State, SyncDoc};
    let (mut sa, mut sb) = (State::new(), State::new());
    for _ in 0..100 {
        let ab = a.doc.sync().generate_sync_message(&mut sa);
        let ba = b.doc.sync().generate_sync_message(&mut sb);
        if ab.is_none() && ba.is_none() {
            return Ok(());
        }
        if let Some(m) = ab {
            b.doc
                .sync()
                .receive_sync_message(&mut sb, Message::decode(&m.encode())?)?;
        }
        if let Some(m) = ba {
            a.doc
                .sync()
                .receive_sync_message(&mut sa, Message::decode(&m.encode())?)?;
        }
    }
    anyhow::bail!("Sync did not settle")
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn edits_preserve_objects_membership_edges_and_roundtrip(ops in prop::collection::vec(operation(),1..50)) {
        let (mut board,mut model) = seed().unwrap();
        for op in &ops { let result = apply(&mut board,&mut model,op); prop_assert!(result.is_ok(), "{op:?}: {result:?}"); }
        let before = snapshot(&board);
        let loaded = Board::load(&board.save()).unwrap();
        prop_assert_eq!(before,snapshot(&loaded));
    }
    #[test]
    fn partitioned_replicas_converge_under_merge_and_reconnect(ops in prop::collection::vec((0_usize..3,operation()),1..35), reversed in any::<bool>()) {
        let (mut original,model) = seed().unwrap(); let bytes = original.save();
        let mut boards: Vec<_> = (0..3).map(|i| { let mut b=Board::load(&bytes).unwrap(); b.doc.set_actor(ActorId::from(&[i+2][..])); b }).collect();
        let mut models = vec![model;3];
        for (i,op) in &ops { let result=apply(&mut boards[*i],&mut models[*i],op); prop_assert!(result.is_ok(), "Replica {i}, {op:?}: {result:?}"); }
        let model = union(&models);
        let mut direct = Board::load(&boards[0].save()).unwrap();
        let order = if reversed { [2,1] } else { [1,2] };
        for i in order { direct.doc.merge(&mut boards[i].doc.clone()).unwrap(); }
        // Fresh sync states model reconnection; each reliable stream carries encoded bytes.
        let mut a=boards.remove(0); let mut b=boards.remove(0); let mut c=boards.remove(0);
        for _ in 0..2 { sync(&mut a,&mut b).unwrap(); sync(&mut b,&mut c).unwrap(); sync(&mut c,&mut a).unwrap(); }
        for board in [&a,&b,&c,&direct] { let result=check(board,&model); prop_assert!(result.is_ok(), "{result:?}"); prop_assert_eq!(snapshot(board),snapshot(&direct)); }
        let before=snapshot(&a); let heads=a.doc.get_heads();
        a.doc.merge(&mut b.doc).unwrap(); a.doc.merge(&mut c.doc).unwrap();
        prop_assert_eq!(before,snapshot(&a)); prop_assert_eq!(heads,a.doc.get_heads());
    }
    #[test]
    fn anchors_are_finite_for_finite_document_positions(
        x in prop_oneof![Just(f32::MAX),Just(-f32::MAX),-10_000_f32..10_000_f32],
        target in prop_oneof![Just(f32::MAX),Just(-f32::MAX),-10_000_f32..10_000_f32],
        shape in 0_usize..3
    ) {
        let bounds=crate::geometry::Bounds { x,y:0.,width:232.,height:158. };
        let point=bounds.anchor([target,100.],Shape::ALL[shape]);
        prop_assert!(point.iter().all(|v|v.is_finite()), "{point:?}");
    }
    #[test]
    fn frame_translation_never_publishes_non_finite_children(delta in prop_oneof![Just(f32::MAX),Just(-f32::MAX)]) {
        let mut board=Board::new().unwrap(); let frame=board.add(Kind::Frame,"Area",0.,0.).unwrap();
        let child=board.add(Kind::Sticky,"Member",50.,80.).unwrap(); board.reparent(&child).unwrap();
        let mut c=board.cards().into_iter().find(|c|c.id==child).unwrap(); c.x=delta; board.edit(&c).unwrap();
        let before=snapshot(&board); let heads=board.doc.get_heads();
        let mut f=board.cards().into_iter().find(|c|c.id==frame).unwrap(); f.x=delta;
        match board.edit(&f) {
            Ok(())=>prop_assert!(board.cards().iter().all(|c|c.x.is_finite() && c.y.is_finite())),
            Err(_)=>{ prop_assert_eq!(before,snapshot(&board)); prop_assert_eq!(heads,board.doc.get_heads()); }
        }
    }
    #[test]
    fn invalid_add_is_rejected_without_mutation(x in prop_oneof![Just(f32::NAN),Just(f32::INFINITY),Just(f32::NEG_INFINITY)]) {
        let mut board=Board::new().unwrap(); let before=snapshot(&board); let heads=board.doc.get_heads();
        prop_assert!(board.add(Kind::Sticky,"Invalid",x,0.).is_err());
        prop_assert_eq!(before,snapshot(&board)); prop_assert_eq!(heads,board.doc.get_heads());
    }
    #[test]
    fn invalid_edit_and_saved_geometry_are_rejected(value in prop_oneof![Just(f32::NAN),Just(f32::INFINITY),Just(-1_f32),Just(0_f32)]) {
        use automerge::{ReadDoc, ROOT, transaction::Transactable};
        let mut board=Board::new().unwrap(); let id=board.add(Kind::Sticky,"Keep this",0.,0.).unwrap();
        let before=snapshot(&board); let heads=board.doc.get_heads();
        let mut card=board.cards().remove(0); card.width=value;
        prop_assert!(board.edit(&card).is_err());
        prop_assert_eq!(before,snapshot(&board)); prop_assert_eq!(heads,board.doc.get_heads());
        let (_,obj)=board.doc.get(ROOT,&id).unwrap().unwrap();
        board.doc.put(obj,"width",value as f64).unwrap();
        prop_assert!(Board::load(&board.save()).is_err());
    }
    #[test]
    fn malformed_saved_relationships_never_become_visible(invalid_kind in 0_u8..3) {
        use automerge::{ROOT, ObjType, transaction::Transactable};
        let mut board=Board::new().unwrap();
        let node=board.add(Kind::Sticky,"Node",0.,0.).unwrap();
        let frame=board.add(Kind::Frame,"Frame",500.,0.).unwrap();
        let endpoint=match invalid_kind { 0=>node.as_str(),1=>frame.as_str(),_=>"card:missing" };
        let edge=board.doc.put_object(ROOT,"edge:invalid",ObjType::Map).unwrap();
        board.doc.put(&edge,"from",node.as_str()).unwrap(); board.doc.put(&edge,"to",endpoint).unwrap();
        board.doc.put_object(edge,"text",ObjType::Text).unwrap();
        let loaded=Board::load(&board.save()).unwrap();
        prop_assert!(loaded.connectors().is_empty());
        prop_assert_eq!(loaded.cards().len(),2);
    }
    #[test]
    fn connector_label_command_cannot_edit_node_text(label in text()) {
        let mut board=Board::new().unwrap(); let id=board.add(Kind::Sticky,"Keep this",0.,0.).unwrap(); let before=snapshot(&board); let heads=board.doc.get_heads();
        prop_assert!(board.label_connector(&id,&label).is_err());
        prop_assert_eq!(before,snapshot(&board)); prop_assert_eq!(heads,board.doc.get_heads());
    }
    #[test]
    fn session_write_failure_does_not_close_board(content in text()) {
        let dir=tempfile::tempdir().unwrap(); let mut ws=Workspace::load(dir.path().into()).unwrap();
        ws.board_mut().unwrap().add(Kind::Sticky,&content,0.,0.).unwrap(); ws.dirty=true;
        std::fs::create_dir(dir.path().join("session.tmp")).unwrap();
        prop_assert!(ws.close(0).is_err());
        prop_assert!(ws.board().is_some()); prop_assert!(ws.entries[0].open);
        prop_assert_eq!(&ws.board().unwrap().cards()[0].text,&content);
    }
    #[test]
    fn workspace_sequences_preserve_saved_content_and_archive_deletions(ops in prop::collection::vec((0_u8..7,any::<u8>(),text()),1..30)) {
        let dir=tempfile::tempdir().unwrap(); let mut ws=Workspace::load(dir.path().into()).unwrap();
        let mut expected:BTreeMap<String,Vec<Card>>=ws.entries.iter().map(|e|(e.board.id(),e.board.cards())).collect();
        for (op,index,text) in ops {
            match op {
                0 => { ws.create().unwrap(); expected.insert(ws.board().unwrap().id(),vec![]); }
                1..=3 if !ws.entries.is_empty() => {
                    let i=index as usize%ws.entries.len();
                    match op {
                        1=>ws.open(i).unwrap(), 2=>ws.close(i).unwrap(),
                        _=>{ let id=ws.entries[i].board.id(); let path=ws.delete(i).unwrap(); let archived=Board::load(&std::fs::read(path).unwrap()).unwrap(); prop_assert_eq!(archived.cards(),expected.remove(&id).unwrap()); }
                    }
                }
                4 => if let Some(board)=ws.board_mut() { board.add(Kind::Sticky,&text,0.,0.).unwrap(); expected.insert(board.id(),board.cards()); ws.dirty=true; },
                5 => { ws.flush().unwrap(); let flags:BTreeMap<_,_>=ws.entries.iter().map(|e|(e.board.id(),e.open)).collect(); let active=ws.board().map(Board::id); ws=Workspace::load(dir.path().into()).unwrap(); prop_assert_eq!(flags,ws.entries.iter().map(|e|(e.board.id(),e.open)).collect()); prop_assert_eq!(active,ws.board().map(Board::id)); }
                _=>ws.flush().unwrap(),
            }
            let actual:BTreeMap<_,_>=ws.entries.iter().map(|e|(e.board.id(),e.board.cards())).collect(); prop_assert_eq!(&actual,&expected);
            prop_assert!(ws.active.is_none_or(|i| i<ws.entries.len() && ws.entries[i].open));
            prop_assert_eq!(ws.active.is_none(),ws.entries.iter().all(|e|!e.open));
        }
    }
}

#[test]
fn reloaded_noop_edit_can_merge_remote_changes() {
    let mut seed = Board::new().unwrap();
    let id = seed.add(Kind::Sticky, "Seed", 0., 0.).unwrap();
    seed.reparent(&id).unwrap();
    let bytes = seed.save();
    let mut local = Board::load(&bytes).unwrap();
    let mut remote = Board::load(&bytes).unwrap();
    local.doc.set_actor(ActorId::from(&b"local"[..]));
    remote.doc.set_actor(ActorId::from(&b"remote"[..]));
    local.reparent(&id).unwrap();
    remote.rename("Remote title").unwrap();
    local.doc.merge(&mut remote.doc).unwrap();
    assert_eq!(local.title(), "Remote title");
}

#[test]
fn invalid_creation_leaves_no_partial_object_or_history() {
    let mut board = Board::new().unwrap();
    let before = snapshot(&board);
    let heads = board.doc.get_heads();
    for (x, y) in [(f32::NAN, 0.), (0., f32::INFINITY), (f32::NEG_INFINITY, 0.)] {
        assert!(board.add(Kind::Sticky, "Invalid", x, y).is_err());
    }
    assert!(board
        .add(Kind::Sticky, &"é".repeat(16_001), 0., 0.)
        .is_err());
    assert_eq!(before, snapshot(&board));
    assert_eq!(heads, board.doc.get_heads());
}
#[test]
fn wrong_target_commands_preserve_node_text_and_metadata() {
    let mut board = Board::new().unwrap();
    let id = board.add(Kind::Sticky, "Original", 0., 0.).unwrap();
    let before = snapshot(&board);
    let heads = board.doc.get_heads();
    assert!(board.label_connector(&id, "Wrong command").is_err());
    assert!(board.delete("title").is_err());
    assert!(board.delete("id").is_err());
    assert_eq!(before, snapshot(&board));
    assert_eq!(heads, board.doc.get_heads());
}
#[test]
fn failed_session_updates_restore_open_and_active_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::load(dir.path().into()).unwrap();
    ws.create().unwrap();
    ws.close(0).unwrap();
    let active = ws.board().unwrap().id();
    std::fs::create_dir(dir.path().join("session.tmp")).unwrap();
    assert!(ws.open(0).is_err());
    assert!(!ws.entries[0].open);
    assert!(ws.close(1).is_err());
    assert!(ws.entries[1].open);
    assert_eq!(ws.board().unwrap().id(), active);
    let reloaded = Workspace::load(dir.path().into()).unwrap();
    assert_eq!(reloaded.board().unwrap().id(), active);
    assert!(
        !reloaded
            .entries
            .iter()
            .find(|e| e.board.id() == ws.entries[0].board.id())
            .unwrap()
            .open
    );
    assert!(ws.open(usize::MAX).is_err());
    assert!(ws.close(usize::MAX).is_err());
    assert!(ws.delete(usize::MAX).is_err());
}
#[test]
fn overflowing_frame_move_rejects_before_any_member_changes() {
    let mut board = Board::new().unwrap();
    let area = board.add(Kind::Frame, "Area", 0., 0.).unwrap();
    let member = board.add(Kind::Sticky, "Member", 50., 80.).unwrap();
    board.reparent(&member).unwrap();
    let mut child = board.cards().into_iter().find(|c| c.id == member).unwrap();
    child.x = f32::MAX;
    board.edit(&child).unwrap();
    let before = snapshot(&board);
    let heads = board.doc.get_heads();
    let mut frame = board.cards().into_iter().find(|c| c.id == area).unwrap();
    frame.x = f32::MAX;
    assert!(board.edit(&frame).is_err());
    assert_eq!(before, snapshot(&board));
    assert_eq!(heads, board.doc.get_heads());
}
#[test]
fn invalid_geometry_cannot_replace_an_existing_saved_board() {
    use automerge::{transaction::Transactable, ReadDoc, ROOT};
    let dir = tempfile::tempdir().unwrap();
    let mut board = Board::new().unwrap();
    let id = board.add(Kind::Sticky, "Saved content", 0., 0.).unwrap();
    crate::storage::save(dir.path(), &mut board).unwrap();
    let path = dir.path().join(format!("{}.automerge", board.id()));
    let bytes = std::fs::read(&path).unwrap();
    let (_, obj) = board.doc.get(ROOT, id).unwrap().unwrap();
    board.doc.put(obj, "width", f64::NAN).unwrap();
    assert!(crate::storage::save(dir.path(), &mut board).is_err());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert!(Board::load(&board.save()).is_err());
    assert_eq!(
        Board::load(&bytes).unwrap().cards()[0].text,
        "Saved content"
    );
}
#[test]
fn extreme_finite_positions_do_not_produce_nan_geometry() {
    let bounds = crate::geometry::Bounds {
        x: -f32::MAX,
        y: 0.,
        width: 232.,
        height: 158.,
    };
    for shape in Shape::ALL {
        assert!(bounds
            .anchor([f32::MAX, 100.], shape)
            .iter()
            .all(|v| v.is_finite()));
    }
    assert!(!crate::geometry::segment_distance([0., 0.], [-f32::MAX, 0.], [f32::MAX, 0.]).is_nan());
}
#[test]
fn schema_two_fixture_from_previous_dependency_keeps_content() {
    let bytes = include_bytes!("../tests/fixtures/schema2-automerge-0.7.4.automerge");
    let mut board = Board::load(bytes).unwrap();
    assert_eq!(board.title(), "Compatibility fixture");
    assert_eq!(board.cards().len(), 3);
    let cards = board.cards();
    let note = cards.iter().find(|c| c.id == "card:note").unwrap();
    assert_eq!(note.text, "é中🙂");
    assert_eq!(note.parent.as_deref(), Some("card:frame"));
    assert_eq!(
        (note.x, note.y, note.width, note.height),
        (50., 80., 232., 158.)
    );
    assert_eq!(
        cards.iter().find(|c| c.id == "card:target").unwrap().shape,
        Shape::Ellipse
    );
    assert_eq!(board.connectors()[0].text, "uses");
    let before = snapshot(&board);
    let loaded = Board::load(&board.save()).unwrap();
    assert_eq!(before, snapshot(&loaded));
}
