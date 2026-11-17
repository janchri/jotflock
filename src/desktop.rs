//! Linux desktop URL integration and single-instance invitation forwarding.
use anyhow::{ensure, Context, Result};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver},
    time::Duration,
};
pub struct Instance {
    pub links: Receiver<String>,
    _lock: fs::File,
}
pub fn instance(root: &Path, link: Option<&str>) -> Result<Option<Instance>> {
    fs::create_dir_all(root)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("app.lock"))?;
    if lock.try_lock().is_err() {
        #[cfg(unix)]
        if let Some(link) = link {
            use std::os::unix::net::UnixStream;
            let mut stream = UnixStream::connect(root.join("app.sock"))
                .context("The app is starting; paste the invitation into its window")?;
            stream.set_write_timeout(Some(Duration::from_secs(1)))?;
            stream.write_all(link.as_bytes())?;
            stream.shutdown(std::net::Shutdown::Write)?;
        }
        return Ok(None);
    }
    let (tx, links) = mpsc::sync_channel(8);
    #[cfg(unix)]
    {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        let socket = root.join("app.sock");
        if socket.exists() {
            fs::remove_file(&socket)?;
        }
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
                let mut bytes = Vec::new();
                if std::io::Read::by_ref(&mut stream)
                    .take(128 * 1024 + 1)
                    .read_to_end(&mut bytes)
                    .is_ok()
                    && bytes.len() <= 128 * 1024
                {
                    if let Ok(link) = String::from_utf8(bytes) {
                        if link.starts_with(crate::sharing::PREFIX) {
                            let _ = tx.try_send(link);
                        }
                    }
                }
            }
        });
    }
    #[cfg(not(unix))]
    drop(tx);
    Ok(Some(Instance { links, _lock: lock }))
}
pub fn install() -> Result<()> {
    let data = directories::BaseDirs::new()
        .context("No desktop data directory")?
        .data_dir()
        .to_path_buf();
    let executable = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .unwrap_or(std::env::current_exe()?);
    let executable = executable.to_string_lossy();
    ensure!(
        !executable.contains(['\n', '\r']),
        "Invalid executable path"
    );
    let quoted = executable
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('`', "\\`")
        .replace('$', "\\$");
    let applications = data.join("applications");
    let icons = data.join("icons/hicolor/256x256/apps");
    fs::create_dir_all(&applications)?;
    fs::create_dir_all(&icons)?;
    fs::write(
        icons.join("jotflock.png"),
        include_bytes!("../assets/jotflock.png"),
    )?;
    fs::write(applications.join("jotflock.desktop"),format!("[Desktop Entry]\nType=Application\nName=Jotflock\nExec=\"{quoted}\" %u\nIcon=jotflock\nCategories=Office;\nTerminal=false\nMimeType=x-scheme-handler/jotflock;\n"))?;
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&applications)
        .output();
    let registration = std::process::Command::new("xdg-mime")
        .args(["default", "jotflock.desktop", "x-scheme-handler/jotflock"])
        .output()
        .context("Desktop entry created; install xdg-utils or paste invitation links in the app")?;
    ensure!(registration.status.success(),"Desktop entry created; register jotflock.desktop as the handler for jotflock links in your desktop settings");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_instance_forwards_without_opening_storage_again() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let first = instance(dir.path(), None)?.unwrap();
        assert!(instance(dir.path(), Some("jotflock://join/test"))?.is_none());
        #[cfg(unix)]
        assert_eq!(
            first.links.recv_timeout(Duration::from_secs(2))?,
            "jotflock://join/test"
        );
        Ok(())
    }
}
