//! Profile lock, local commands and Linux invitation registration.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
const REQUEST_LIMIT: u64 = 256 * 1024;

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Success { ok: bool, result: Value },
    Failure { ok: bool, error: String },
}
impl From<Result<Value>> for Reply {
    fn from(result: Result<Value>) -> Self {
        match result {
            Ok(result) => Self::Success { ok: true, result },
            Err(error) => Self::Failure {
                ok: false,
                error: format!("{error:#}"),
            },
        }
    }
}
impl Reply {
    pub fn ok(&self) -> bool {
        matches!(self, Self::Success { .. })
    }
    pub fn pretty(&self) -> String {
        match self {
            Self::Success { result, .. } => {
                serde_json::to_string_pretty(result).unwrap_or_default()
            }
            Self::Failure { error, .. } => error.clone(),
        }
    }
}

pub struct Request {
    pub args: Vec<String>,
    #[cfg(unix)]
    stream: UnixStream,
}
impl Request {
    pub fn respond(self, reply: &Reply) {
        #[cfg(unix)]
        let _ = write_reply(self.stream, reply);
        #[cfg(not(unix))]
        let _ = reply;
    }
}

#[cfg(unix)]
fn write_reply(mut stream: UnixStream, reply: &Reply) -> Result<()> {
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let bytes = serde_json::to_vec(reply)?;
    stream.write_all(&bytes)?;
    stream.shutdown(std::net::Shutdown::Write)?;
    Ok(())
}

#[cfg(unix)]
fn request(stream: &mut UnixStream) -> Result<Vec<u8>> {
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let mut bytes = Vec::new();
    stream.take(REQUEST_LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= REQUEST_LIMIT, "Command is too large");
    Ok(bytes)
}

pub fn send(root: &Path, args: &[String]) -> Result<Reply> {
    #[cfg(unix)]
    {
        let bytes = serde_json::to_vec(args)?;
        ensure!(bytes.len() as u64 <= REQUEST_LIMIT, "Command is too large");
        let mut stream = UnixStream::connect(root.join("app.sock"))
            .context("Cannot reach the active Jotflock instance for this data directory")?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.write_all(&bytes)?;
        stream.shutdown(std::net::Shutdown::Write)?;
        serde_json::from_reader(stream)
            .context("No valid command response from the active instance")
    }
    #[cfg(not(unix))]
    {
        let _ = (root, args);
        anyhow::bail!("Commands to a running instance require Unix sockets")
    }
}
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    sync::mpsc::{self, Receiver},
    time::Duration,
};
pub struct Instance {
    pub links: Receiver<String>,
    pub commands: Receiver<Request>,
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
    if let Err(error) = lock.try_lock() {
        match error {
            fs::TryLockError::WouldBlock => {}
            fs::TryLockError::Error(error) => return Err(error).context("Locking app profile"),
        }
        #[cfg(unix)]
        if let Some(link) = link {
            let mut stream = UnixStream::connect(root.join("app.sock"))
                .context("The app is starting; paste the invitation into its window")?;
            stream.set_write_timeout(Some(Duration::from_secs(1)))?;
            stream.write_all(link.as_bytes())?;
            stream.shutdown(std::net::Shutdown::Write)?;
        }
        return Ok(None);
    }
    let (tx, links) = mpsc::sync_channel(8);
    let (command_tx, commands) = mpsc::sync_channel(8);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let socket = root.join("app.sock");
        if socket.exists() {
            fs::remove_file(&socket)?;
        }
        let listener = UnixListener::bind(&socket)?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let Ok(bytes) = request(&mut stream) else {
                    continue;
                };
                if bytes.starts_with(crate::sharing::PREFIX.as_bytes()) {
                    if let Ok(link) = String::from_utf8(bytes) {
                        let _ = tx.try_send(link);
                    }
                    continue;
                }
                match serde_json::from_slice::<Vec<String>>(&bytes) {
                    Ok(args) => {
                        if let Err(error) = command_tx.try_send(Request { args, stream }) {
                            let (mpsc::TrySendError::Full(request)
                            | mpsc::TrySendError::Disconnected(request)) = error;
                            request.respond(
                                &Err(anyhow::anyhow!("Jotflock command queue is unavailable"))
                                    .into(),
                            );
                        }
                    }
                    Err(error) => {
                        let _ = write_reply(stream, &Err(error.into()).into());
                    }
                }
            }
        });
    }
    #[cfg(not(unix))]
    drop((tx, command_tx, link));
    Ok(Some(Instance {
        links,
        commands,
        _lock: lock,
    }))
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
