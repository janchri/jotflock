//! Signed, device-bound board membership. Authorization is separate from canvas data.
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use iroh::Signature;
use iroh::{EndpointAddr, EndpointId, SecretKey};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const LIFETIME: u64 = 24 * 60 * 60;
pub const MAX_PEERS: usize = 16;
const MAX_LINK: usize = 128 * 1024;
pub const PREFIX: &str = "jotflock://join/";
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn sign<T: Serialize>(key: &SecretKey, domain: &str, value: &T) -> Result<Vec<u8>> {
    let bytes = payload(domain, value)?;
    Ok(key.sign(&bytes).to_bytes().to_vec())
}
fn payload<T: Serialize>(domain: &str, value: &T) -> Result<Vec<u8>> {
    let mut bytes = domain.as_bytes().to_vec();
    bytes.push(0);
    bytes.extend(serde_json::to_vec(value)?);
    Ok(bytes)
}
fn verify<T: Serialize>(key: &str, domain: &str, value: &T, sig: &[u8]) -> Result<()> {
    let key: EndpointId = key.parse()?;
    key.verify(&payload(domain, value)?, &Signature::try_from(sig)?)?;
    Ok(())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantBody {
    board: String,
    issuer: String,
    member: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    body: GrantBody,
    signature: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub root: String,
    pub grants: Vec<Grant>,
}
impl Proof {
    pub fn founder(key: &SecretKey) -> Self {
        Self {
            root: key.public().to_string(),
            grants: vec![],
        }
    }
    pub fn member(&self, board: &str, root: &str) -> Result<String> {
        ensure!(
            self.root == root && self.grants.len() <= 32,
            "Invalid membership root or depth"
        );
        let _: EndpointId = root.parse()?;
        let mut member = root.to_owned();
        let mut seen = std::collections::HashSet::from([member.clone()]);
        for grant in &self.grants {
            ensure!(
                grant.body.board == board && grant.body.issuer == member,
                "Broken membership chain"
            );
            verify(&member, "jotflock/member/v1", &grant.body, &grant.signature)?;
            let _: EndpointId = grant.body.member.parse()?;
            ensure!(
                seen.insert(grant.body.member.clone()),
                "Cyclic membership chain"
            );
            member = grant.body.member.clone();
        }
        Ok(member)
    }
    pub fn admit(&self, key: &SecretKey, board: &str, member: EndpointId) -> Result<Self> {
        ensure!(
            self.member(board, &self.root)? == key.public().to_string(),
            "Not an admitted editor"
        );
        if member == key.public() {
            return Ok(self.clone());
        }
        let body = GrantBody {
            board: board.into(),
            issuer: key.public().to_string(),
            member: member.to_string(),
        };
        let mut proof = self.clone();
        proof.grants.push(Grant {
            signature: sign(key, "jotflock/member/v1", &body)?,
            body,
        });
        proof.member(board, &self.root)?;
        Ok(proof)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Peer {
    pub addr: EndpointAddr,
    pub proof: Proof,
}
impl Peer {
    pub fn validate(&self, board: &str, root: &str) -> Result<()> {
        ensure!(self.addr.addrs.len() <= 16, "Too many addresses");
        ensure!(
            self.proof.member(board, root)? == self.addr.id.to_string(),
            "Peer identity mismatch"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InviteBody {
    pub version: u8,
    pub board: String,
    pub root: String,
    pub issuer: String,
    pub issued: u64,
    pub expires: u64,
    nonce: [u8; 32],
    pub proof: Proof,
    pub peers: Vec<Peer>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invitation {
    pub body: InviteBody,
    signature: Vec<u8>,
}
impl Invitation {
    pub fn create(
        key: &SecretKey,
        board: &str,
        proof: Proof,
        peers: Vec<Peer>,
        time: u64,
    ) -> Result<Self> {
        let body = InviteBody {
            version: 1,
            board: board.into(),
            root: proof.root.clone(),
            issuer: key.public().to_string(),
            issued: time,
            expires: time.checked_add(LIFETIME).context("Invalid time")?,
            nonce: rand::rng().random(),
            proof,
            peers,
        };
        let invite = Self {
            signature: sign(key, "jotflock/invite/v1", &body)?,
            body,
        };
        invite.validate(time)?;
        Ok(invite)
    }
    pub fn validate(&self, time: u64) -> Result<()> {
        let b = &self.body;
        ensure!(b.version == 1, "Unsupported invitation version");
        uuid::Uuid::parse_str(&b.board)?;
        ensure!(
            b.issued <= time
                && b.expires.checked_sub(b.issued) == Some(LIFETIME)
                && time < b.expires,
            "Invitation expired or device clock is incorrect"
        );
        ensure!(
            b.proof.member(&b.board, &b.root)? == b.issuer,
            "Invalid invitation issuer"
        );
        verify(&b.issuer, "jotflock/invite/v1", b, &self.signature)?;
        ensure!(
            !b.peers.is_empty() && b.peers.len() <= MAX_PEERS,
            "Invalid invitation peers"
        );
        for peer in &b.peers {
            peer.validate(&b.board, &b.root)?;
        }
        Ok(())
    }
    pub fn link(&self) -> Result<String> {
        let value = format!("{PREFIX}{}", B64.encode(serde_json::to_vec(self)?));
        ensure!(value.len() <= MAX_LINK, "Invitation is too large");
        Ok(value)
    }
    pub fn parse(link: &str, time: u64) -> Result<Self> {
        ensure!(link.len() <= MAX_LINK, "Invitation is too large");
        let value = link
            .trim()
            .strip_prefix(PREFIX)
            .context("Paste a Jotflock invitation link")?;
        let invite: Self = serde_json::from_slice(&B64.decode(value)?)?;
        invite.validate(time)?;
        Ok(invite)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authorization {
    pub board: String,
    pub proof: Proof,
    pub peers: Vec<Peer>,
}
pub fn private_write(path: &Path, bytes: &[u8]) -> Result<()> {
    crate::storage::write_atomic(path, bytes)
}
pub fn load_key(root: &Path) -> Result<SecretKey> {
    let folder = root.join("network");
    fs::create_dir_all(&folder)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    }
    let path = folder.join("device.key");
    if path.exists() {
        let bytes: [u8; 32] = fs::read(path)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid device key; restore it from backup"))?;
        return Ok(SecretKey::from_bytes(&bytes));
    }
    let key = SecretKey::generate();
    private_write(&path, &key.to_bytes())?;
    Ok(key)
}
fn auth_path(root: &Path, board: &str) -> Result<PathBuf> {
    uuid::Uuid::parse_str(board)?;
    Ok(root
        .join("network")
        .join("access")
        .join(format!("{board}.json")))
}
pub fn previous_authority(root: &Path, board: &str) -> Result<Option<String>> {
    uuid::Uuid::parse_str(board)?;
    let path = root.join("network").join(format!("{board}.json"));
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let auth: Authorization = serde_json::from_slice(&bytes)?;
    let _: EndpointId = auth.proof.root.parse()?;
    ensure!(
        auth.board == board,
        "Stored board authority has a different board ID"
    );
    Ok(Some(auth.proof.root))
}
pub fn save_auth(root: &Path, auth: &Authorization) -> Result<()> {
    private_write(&auth_path(root, &auth.board)?, &serde_json::to_vec(auth)?)
}
pub fn load_auth(root: &Path, board: &str, key: &SecretKey) -> Result<Option<Authorization>> {
    let path = auth_path(root, board)?;
    if !path.exists() {
        return Ok(None);
    }
    let auth: Authorization = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        auth.board == board
            && auth.proof.member(board, &auth.proof.root)? == key.public().to_string(),
        "Device membership does not match this board"
    );
    for p in &auth.peers {
        p.validate(board, &auth.proof.root)?;
    }
    Ok(Some(auth))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn delegation_and_expiry_are_independent() -> Result<()> {
        let a = SecretKey::generate();
        let b = SecretKey::generate();
        let c = SecretKey::generate();
        let board = uuid::Uuid::new_v4().to_string();
        let proof = Proof::founder(&a).admit(&a, &board, b.public())?;
        let peer = Peer {
            addr: EndpointAddr::new(b.public()),
            proof: proof.clone(),
        };
        let invite = Invitation::create(&b, &board, proof.clone(), vec![peer], 100)?;
        assert!(invite.validate(100 + LIFETIME).is_err());
        let invited = proof.admit(&b, &board, c.public())?;
        assert_eq!(
            invited.member(&board, &a.public().to_string())?,
            c.public().to_string()
        );
        let mut tampered = invite.clone();
        tampered.body.expires += 1;
        assert!(tampered.validate(101).is_err());
        let link = invite.link()?;
        assert!(Invitation::parse(&link, 101).is_ok());
        Ok(())
    }
    #[test]
    fn forged_membership_and_wrong_board_are_rejected() -> Result<()> {
        let a = SecretKey::generate();
        let b = SecretKey::generate();
        let board = uuid::Uuid::new_v4().to_string();
        let mut p = Proof::founder(&a).admit(&a, &board, b.public())?;
        assert!(p.member("other", &a.public().to_string()).is_err());
        p.grants[0].body.member = a.public().to_string();
        assert!(p.member(&board, &a.public().to_string()).is_err());
        Ok(())
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn device_and_membership_survive_restart() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let key = load_key(dir.path())?;
        let board = uuid::Uuid::new_v4().to_string();
        save_auth(
            dir.path(),
            &Authorization {
                board: board.clone(),
                proof: Proof::founder(&key),
                peers: vec![],
            },
        )?;
        let again = load_key(dir.path())?;
        assert_eq!(key.public(), again.public());
        assert!(load_auth(dir.path(), &board, &again)?.is_some());
        assert!(load_auth(dir.path(), &board, &SecretKey::generate()).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(dir.path().join("network/device.key"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        Ok(())
    }
}
