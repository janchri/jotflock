use crate::document::Board;
use anyhow::{anyhow, ensure, Context, Result};
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
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
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
    if path.exists() {
        fs::copy(&path, path.with_extension("bak"))?;
    }
    write_atomic(&path, &board.save()).context("Replacing saved board")
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
            let board = Board::load(&fs::read(&p)?)
                .with_context(|| format!("Cannot open {}; original file preserved", p.display()))?;
            ensure!(
                p.file_stem().and_then(|stem| stem.to_str()) == Some(board.id().as_str()),
                "Board ID does not match filename {}; original file preserved",
                p.display()
            );
            Ok(board)
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_atomic_writes_keep_complete_private_files() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("authorization.json");
        std::thread::scope(|scope| {
            for value in 0..8_u8 {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..8 {
                        write_atomic(path, &vec![value; 4096]).unwrap();
                    }
                });
            }
        });
        let bytes = fs::read(&path)?;
        assert_eq!(bytes.len(), 4096);
        assert!(bytes.iter().all(|b| *b == bytes[0]));
        assert_eq!(fs::read_dir(dir.path())?.count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        }
        Ok(())
    }
    #[test]
    fn mismatched_filename_is_rejected_without_overwriting_boards() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let mut board = Board::new()?;
        save(dir.path(), &mut board)?;
        let original = dir.path().join(format!("{}.automerge", board.id()));
        let wrong = dir
            .path()
            .join(format!("{}.automerge", uuid::Uuid::new_v4()));
        fs::copy(&original, &wrong)?;
        let before = fs::read(&original)?;
        assert!(load_all(dir.path()).is_err());
        assert_eq!(fs::read(original)?, before);
        assert_eq!(fs::read(wrong)?, before);
        Ok(())
    }
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
