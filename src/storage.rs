use crate::document::Board;
use anyhow::{anyhow, Context, Result};
use directories::ProjectDirs;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

pub fn root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("JOTFLOCK_DATA_DIR") {
        return Ok(path.into());
    }
    Ok(ProjectDirs::from("org", "jotflock", "jotflock")
        .ok_or_else(|| anyhow!("Cannot find data directory"))?
        .data_local_dir()
        .to_path_buf())
}
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let root = path
        .parent()
        .ok_or_else(|| anyhow!("Missing parent directory"))?;
    fs::create_dir_all(root)?;
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temp, path)?;
    #[cfg(unix)]
    fs::File::open(root)?.sync_all()?;
    Ok(())
}

pub fn trash(root: &Path, id: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(id).context("Invalid board ID")?;
    let folder = root.join("trash");
    fs::create_dir_all(&folder)?;
    let target = folder.join(format!("{id}-{}.automerge", uuid::Uuid::new_v4()));
    fs::rename(root.join(format!("{id}.automerge")), &target).context("Moving board to trash")?;
    Ok(target)
}

pub fn save(root: &Path, board: &mut Board) -> Result<()> {
    board.validate()?;
    fs::create_dir_all(root)?;
    let path = root.join(format!("{}.automerge", board.id()));
    let temp = path.with_extension("tmp");
    let mut file = fs::File::create(&temp)?;
    file.write_all(&board.save())?;
    file.sync_all()?;
    if path.exists() {
        fs::copy(&path, path.with_extension("bak"))?;
    }
    fs::rename(&temp, &path).context("Replacing saved board")?;
    #[cfg(unix)]
    fs::File::open(root)?.sync_all()?;
    Ok(())
}
pub fn load_all(root: &Path) -> Result<Vec<Board>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<_> = fs::read_dir(root)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|v| v.path())
        .filter(|p| p.extension().is_some_and(|e| e == "automerge"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            Board::load(&fs::read(&p)?)
                .with_context(|| format!("Cannot open {}; original file preserved", p.display()))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn multiple_boards_survive_restart() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut a = Board::new()?;
        a.rename("Architecture")?;
        save(dir.path(), &mut a)?;
        let mut b = Board::new()?;
        b.rename("Brainstorm")?;
        save(dir.path(), &mut b)?;
        a.rename("Architecture v2")?;
        save(dir.path(), &mut a)?;
        let boards = load_all(dir.path())?;
        assert_eq!(boards.len(), 2);
        assert!(boards.iter().any(|b| b.title() == "Architecture v2"));
        assert!(dir.path().join(format!("{}.bak", a.id())).exists());
        Ok(())
    }
}
