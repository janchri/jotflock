//! Local board lifecycle. Closing preserves a saved board; deleting archives its file.
use crate::{document::Board, storage};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

pub struct Entry {
    pub board: Board,
    pub open: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Session {
    open: Vec<String>,
    active: Option<String>,
}
pub struct Workspace {
    pub entries: Vec<Entry>,
    pub active: Option<usize>,
    pub dirty: bool,
    root: PathBuf,
}
impl Workspace {
    pub fn load(root: PathBuf) -> Result<Self> {
        let boards = storage::load_all(&root)?;
        let session_path = root.join("session.json");
        let session: Option<Session> = if session_path.exists() {
            Some(
                serde_json::from_slice(&fs::read(session_path)?)
                    .context("Cannot read board session; saved boards are preserved")?,
            )
        } else {
            None
        };
        let entries: Vec<_> = boards
            .into_iter()
            .map(|board| {
                let open = session
                    .as_ref()
                    .is_none_or(|s| s.open.contains(&board.id()));
                Entry { board, open }
            })
            .collect();
        let active = session
            .as_ref()
            .and_then(|s| s.active.as_ref())
            .and_then(|id| entries.iter().position(|e| e.open && e.board.id() == *id))
            .or_else(|| entries.iter().position(|e| e.open));
        let mut ws = Self {
            entries,
            active,
            dirty: false,
            root,
        };
        if ws.entries.is_empty() && session.is_none() {
            ws.create()?;
        }
        Ok(ws)
    }
    pub fn board(&self) -> Option<&Board> {
        self.active.map(|i| &self.entries[i].board)
    }
    pub fn board_mut(&mut self) -> Option<&mut Board> {
        self.active.map(|i| &mut self.entries[i].board)
    }
    pub fn receive(&mut self, bytes: &[u8]) -> Result<usize> {
        self.flush()?;
        let mut incoming = Board::load(bytes)?;
        let id = incoming.id();
        let index = if let Some(index) = self.entries.iter().position(|e| e.board.id() == id) {
            let mut candidate = Board {
                doc: self.entries[index].board.doc.clone(),
            };
            candidate.doc.merge(&mut incoming.doc)?;
            candidate.validate()?;
            crate::storage::save(&self.root, &mut candidate)?;
            self.entries[index].board = candidate;
            index
        } else {
            crate::storage::save(&self.root, &mut incoming)?;
            self.entries.push(Entry {
                board: incoming,
                open: true,
            });
            self.entries.len() - 1
        };
        self.entries[index].open = true;
        self.active = Some(index);
        self.session()?;
        Ok(index)
    }
    pub fn flush(&mut self) -> Result<()> {
        if self.dirty {
            if let Some(i) = self.active {
                storage::save(&self.root, &mut self.entries[i].board)?;
            }
            self.dirty = false;
        }
        Ok(())
    }
    fn session(&self) -> Result<()> {
        let session = Session {
            open: self
                .entries
                .iter()
                .filter(|e| e.open)
                .map(|e| e.board.id())
                .collect(),
            active: self.board().map(Board::id),
        };
        storage::write_atomic(
            &self.root.join("session.json"),
            &serde_json::to_vec(&session)?,
        )
    }
    pub fn create(&mut self) -> Result<()> {
        self.create_board(None)
    }
    pub fn create_board(&mut self, template: Option<usize>) -> Result<()> {
        self.flush()?;
        let mut board = Board::new()?;
        if let Some(index) = template {
            let name = crate::templates::NAMES
                .get(index)
                .context("Unknown template")?;
            crate::templates::insert(&mut board, index)?;
            board.rename(name)?;
        }
        storage::save(&self.root, &mut board)?;
        self.entries.push(Entry { board, open: true });
        self.active = Some(self.entries.len() - 1);
        self.session()
    }
    pub fn rename(&mut self, index: usize, title: &str) -> Result<()> {
        self.flush()?;
        let entry = self.entries.get_mut(index).context("Unknown board")?;
        let mut candidate = Board {
            doc: entry.board.doc.clone(),
        };
        candidate.rename(title)?;
        storage::save(&self.root, &mut candidate)?;
        entry.board = candidate;
        Ok(())
    }

    pub fn open(&mut self, index: usize) -> Result<()> {
        ensure!(index < self.entries.len(), "Unknown board");
        self.flush()?;
        let previous = (self.entries[index].open, self.active);
        self.entries[index].open = true;
        self.active = Some(index);
        if let Err(error) = self.session() {
            self.entries[index].open = previous.0;
            self.active = previous.1;
            return Err(error);
        }
        Ok(())
    }
    pub fn close(&mut self, index: usize) -> Result<()> {
        ensure!(index < self.entries.len(), "Unknown board");
        self.flush()?;
        let previous = (self.entries[index].open, self.active);
        self.entries[index].open = false;
        if self.active == Some(index) {
            self.active = self.entries.iter().position(|e| e.open);
        }
        if let Err(error) = self.session() {
            self.entries[index].open = previous.0;
            self.active = previous.1;
            return Err(error);
        }
        Ok(())
    }
    pub fn delete(&mut self, index: usize) -> Result<PathBuf> {
        ensure!(index < self.entries.len(), "Unknown board");
        self.flush()?;
        let path = storage::trash(&self.root, &self.entries[index].board.id())?;
        self.entries.remove(index);
        self.active = match self.active {
            Some(i) if i == index => self.entries.iter().position(|e| e.open),
            Some(i) if i > index => Some(i - 1),
            other => other,
        };
        self.session()?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Kind;
    #[test]
    fn close_last_board_reopen_and_restart_preserve_content() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut ws = Workspace::load(dir.path().into())?;
        ws.board_mut()
            .unwrap()
            .add(Kind::Sticky, "Keep me", 20., 30.)?;
        ws.dirty = true;
        ws.close(0)?;
        assert!(ws.board().is_none());
        let mut ws = Workspace::load(dir.path().into())?;
        assert!(ws.board().is_none());
        ws.open(0)?;
        assert_eq!(ws.board().unwrap().cards()[0].text, "Keep me");
        Ok(())
    }
    #[test]
    fn deleting_boards_keeps_selection_valid_and_does_not_resurrect() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut ws = Workspace::load(dir.path().into())?;
        ws.create()?;
        let second = ws.board().unwrap().id();
        let archived = ws.delete(0)?;
        assert_eq!(ws.board().unwrap().id(), second);
        assert!(Board::load(&fs::read(archived)?).is_ok());
        ws.delete(0)?;
        assert!(ws.board().is_none());
        let ws = Workspace::load(dir.path().into())?;
        assert!(ws.entries.is_empty());
        Ok(())
    }
    #[test]
    fn failed_save_prevents_closing_unsaved_board() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("boards");
        let mut ws = Workspace::load(root.clone())?;
        ws.dirty = true;
        // A directory at the temporary-file path forces a real filesystem failure.
        fs::create_dir(root.join(format!("{}.tmp", ws.board().unwrap().id())))?;
        assert!(ws.close(0).is_err());
        assert!(ws.board().is_some());
        assert!(ws.dirty);
        Ok(())
    }
}
