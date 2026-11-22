use super::*;

#[test]
fn failed_close_keeps_unsaved_edits_and_can_be_retried() -> Result<()> {
    let profile = tempfile::tempdir()?;
    let instance = desktop::instance(profile.path(), None)?.unwrap();
    let mut app = Canvas::new(profile.path().into(), instance, None, None, false)?;
    app.workspace
        .board_mut()
        .unwrap()
        .add(document::Kind::Sticky, "Keep this edit", 0., 0.)?;
    app.workspace.dirty = true;
    let id = app.workspace.board().unwrap().id();
    let blocked_backup = profile.path().join(format!("{id}.bak"));
    std::fs::create_dir(&blocked_backup)?;
    let context = egui::Context::default();
    let request_close = || {
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        input
    };
    let output = context.run(request_close(), |ctx| app.save_before_close(ctx));
    assert!(output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    assert!(app.workspace.dirty);
    assert!(app.status.contains("Could not close"));
    std::fs::remove_dir(blocked_backup)?;
    let output = context.run(request_close(), |ctx| app.save_before_close(ctx));
    assert!(!output.viewport_output[&egui::ViewportId::ROOT]
        .commands
        .contains(&egui::ViewportCommand::CancelClose));
    assert!(!app.workspace.dirty);
    let saved = storage::load_all(profile.path())?;
    assert_eq!(saved[0].cards()[0].text, "Keep this edit");
    Ok(())
}
