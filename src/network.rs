//! Encrypted iroh sessions. A background actor owns replicas and peer connections;
//! only authenticated members reach the bounded document synchronization protocol.
use crate::{
    document::Board,
    sharing::{self, Authorization, Invitation, Peer, Proof, MAX_PEERS},
};
use anyhow::{anyhow, ensure, Context, Result};
use automerge::{
    sync::{Message, State, SyncDoc},
    ReadDoc, ROOT,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use iroh::{
    endpoint::{presets, Connection, RecvStream, SendStream},
    Endpoint, EndpointId, RelayMode, RelayUrl, SecretKey,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc as channel, Mutex, RwLock, Semaphore};
const ALPN: &[u8] = b"jotflock/board/1";
const MAX_DOC: usize = 4 * 1024 * 1024;
const MAX_FRAME: usize = 8 * 1024 * 1024;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Presence {
    pub name: String,
    pub cursor: Option<[f32; 2]>,
    pub selected: Option<String>,
}
impl Presence {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.name.chars().count() <= 64
                && self.selected.as_ref().is_none_or(|s| s.len() <= 128)
                && self
                    .cursor
                    .is_none_or(|p| p.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.)),
            "Invalid presence"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Credential {
    Invite(Invitation),
    Member(Proof),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Wire {
    Hello {
        version: u8,
        board: String,
        root: String,
        addr: iroh::EndpointAddr,
        credential: Credential,
    },
    Welcome {
        admitted: Proof,
        server: Peer,
        peers: Vec<Peer>,
    },
    Data {
        message: Option<String>,
        peers: Vec<Peer>,
        presence: Presence,
    },
    Reject(String),
}
async fn send(stream: &mut SendStream, value: &Wire) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "Network frame exceeds limit");
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .await?;
    stream.write_all(&bytes).await?;
    Ok(())
}
async fn recv(stream: &mut RecvStream) -> Result<Wire> {
    let mut size = [0; 4];
    stream.read_exact(&mut size).await?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(size > 0 && size <= MAX_FRAME, "Invalid network frame size");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
async fn receive(stream: &mut RecvStream) -> Result<Wire> {
    tokio::time::timeout(Duration::from_secs(20), recv(stream))
        .await
        .context("Peer stopped responding")?
}

pub enum Event {
    Status(u64, String),
    Invitation(u64, String),
    Document(u64, Vec<u8>),
    Fatal(String),
    Failed(u64, String, bool),
    Presence(u64, Vec<(String, Presence)>),
}
enum Command {
    Share {
        epoch: u64,
        bytes: Vec<u8>,
    },
    Join {
        epoch: u64,
        ticket: Box<Invitation>,
        local: Option<Vec<u8>>,
    },
    Publish(Vec<u8>),
    Invite,
    Refresh,
    Stop,
    Shutdown,
}
pub struct Network {
    tx: channel::Sender<Command>,
    pub events: Receiver<Event>,
    pub epoch: u64,
    pub board: Option<String>,
    presence: tokio::sync::watch::Sender<Presence>,
}
impl Network {
    pub fn start(root: PathBuf) -> Self {
        let (tx, mut rx) = channel::channel(8);
        let (events, receiver) = mpsc::sync_channel(8);
        let (presence, presence_rx) = tokio::sync::watch::channel(Presence::default());
        std::thread::spawn(move || {
            let failures = events.clone();
            let run = || -> Result<()> {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()?;
                runtime.block_on(async move{
                    let hub=Hub::new(root,events.clone(),false).await?;let accepting=hub.clone();let accept=tokio::spawn(async move{accepting.accept().await;});
                    let mut tick=tokio::time::interval(Duration::from_millis(300));
                    loop{tokio::select!{
                        command=rx.recv()=>{let Some(command)=command else{break;};let starting=match &command{Command::Share{epoch,..}|Command::Join{epoch,..}=>Some(*epoch),_=>None};let result=match command{
                            Command::Share{epoch,bytes}=>hub.share(epoch,bytes).await,
                            Command::Join{epoch,ticket,local}=>hub.join(epoch,*ticket,local).await,
                            Command::Publish(bytes)=>hub.publish(bytes).await,
                            Command::Invite=>hub.invitation().await.map(|_|()),
                            Command::Refresh=>{if let Some(room)=hub.room.read().await.clone(){room.last_heads.lock().await.clear();}Ok(())},
                            Command::Stop=>{hub.stop().await;Ok(())},Command::Shutdown=>break,
                        };if let Err(error)=result{let epoch=starting.unwrap_or(hub.room.read().await.as_ref().map(|r|r.epoch).unwrap_or(0));if starting.is_some(){hub.stop().await;}let _=events.try_send(Event::Failed(epoch,format!("Sharing error: {error}"),starting.is_some()));}},
                        _=tick.tick()=>{let current=presence_rx.borrow().clone();*hub.presence.write().await=current;hub.maintain().await;},
                    }}
                    hub.stop().await;accept.abort();hub.endpoint.close().await;Ok::<_,anyhow::Error>(())
                })
            };
            if let Err(error) = run() {
                let _ = failures.try_send(Event::Fatal(format!("Networking unavailable: {error}")));
            }
        });
        Self {
            tx,
            events: receiver,
            epoch: 0,
            board: None,
            presence,
        }
    }
    pub fn presence(&self, presence: Presence) {
        self.presence.send_replace(presence);
    }
    pub fn share(&mut self, id: String, bytes: Vec<u8>) -> Result<()> {
        if self.board.as_ref() == Some(&id) {
            self.tx.try_send(Command::Publish(bytes))?;
            self.tx.try_send(Command::Invite)?;
        } else {
            let epoch = self.epoch + 1;
            self.tx.try_send(Command::Share { epoch, bytes })?;
            self.epoch = epoch;
            self.board = Some(id);
        }
        Ok(())
    }
    pub fn join(&mut self, ticket: Invitation, local: Option<Vec<u8>>) -> Result<()> {
        let epoch = self.epoch + 1;
        let id = ticket.body.board.clone();
        self.tx.try_send(Command::Join {
            epoch,
            ticket: Box::new(ticket),
            local,
        })?;
        self.epoch = epoch;
        self.board = Some(id);
        Ok(())
    }
    pub fn publish(&self, bytes: Vec<u8>) -> Result<()> {
        self.tx.try_send(Command::Publish(bytes))?;
        Ok(())
    }
    pub fn refresh(&self) -> Result<()> {
        self.tx.try_send(Command::Refresh)?;
        Ok(())
    }
    pub fn stop(&mut self) -> Result<()> {
        self.tx.try_send(Command::Stop)?;
        self.epoch += 1;
        self.board = None;
        Ok(())
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        let _ = self.tx.try_send(Command::Shutdown);
    }
}
struct Room {
    epoch: u64,
    id: String,
    root: String,
    doc: Mutex<Board>,
    proof: RwLock<Option<Proof>>,
    ticket: RwLock<Option<Invitation>>,
    peers: RwLock<BTreeMap<EndpointId, Peer>>,
    connections: Mutex<BTreeMap<EndpointId, Connection>>,
    dialing: Mutex<HashSet<EndpointId>>,
    alive: AtomicBool,
    last_heads: Mutex<Vec<automerge::ChangeHash>>,
    presence: Mutex<BTreeMap<EndpointId, (Presence, std::time::Instant)>>,
}
struct Hub {
    endpoint: Endpoint,
    key: SecretKey,
    folder: PathBuf,
    events: SyncSender<Event>,
    room: RwLock<Option<Arc<Room>>>,
    slots: Arc<Semaphore>,
    internet: bool,
    presence: RwLock<Presence>,
}
impl Hub {
    async fn new(folder: PathBuf, events: SyncSender<Event>, local: bool) -> Result<Arc<Self>> {
        Self::bind(
            folder,
            events,
            local,
            std::env::var_os("JOTFLOCK_RELAY_ONLY").is_some(),
        )
        .await
    }
    async fn bind(
        folder: PathBuf,
        events: SyncSender<Event>,
        local: bool,
        relay_only: bool,
    ) -> Result<Arc<Self>> {
        let key = sharing::load_key(&folder)?;
        let builder = if local {
            Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled)
        } else {
            Endpoint::builder(presets::N0)
        };
        let mut builder = builder.secret_key(key.clone()).alpns(vec![ALPN.to_vec()]);
        if !local {
            if let Ok(url) = std::env::var("JOTFLOCK_RELAY_URL") {
                builder = builder.relay_mode(RelayMode::Custom(
                    [url.parse::<RelayUrl>()?].into_iter().collect(),
                ));
            }
        }
        if relay_only && !local {
            builder = builder.clear_ip_transports();
        }
        let endpoint = builder.bind().await?;
        Ok(Arc::new(Self {
            endpoint,
            key,
            folder,
            events,
            room: RwLock::new(None),
            slots: Arc::new(Semaphore::new(32)),
            internet: !local,
            presence: RwLock::new(Presence::default()),
        }))
    }
    fn status(&self, room: &Room, text: impl Into<String>) {
        let _ = self.events.try_send(Event::Status(room.epoch, text.into()));
    }
    async fn stop(&self) {
        if let Some(room) = self.room.write().await.take() {
            room.alive.store(false, Ordering::Release);
            for conn in room.connections.lock().await.values() {
                conn.close(0_u32.into(), b"session closed");
            }
        }
    }
    async fn make_room(
        &self,
        epoch: u64,
        id: String,
        root: String,
        doc: Board,
        auth: Option<Authorization>,
        ticket: Option<Invitation>,
    ) -> Arc<Room> {
        let peers = auth
            .as_ref()
            .map(|a| a.peers.clone())
            .unwrap_or_default()
            .into_iter()
            .map(|p| (p.addr.id, p))
            .collect();
        Arc::new(Room {
            epoch,
            id,
            root,
            doc: Mutex::new(doc),
            proof: RwLock::new(auth.map(|a| a.proof)),
            ticket: RwLock::new(ticket),
            peers: RwLock::new(peers),
            connections: Mutex::new(BTreeMap::new()),
            dialing: Mutex::new(HashSet::new()),
            alive: AtomicBool::new(true),
            last_heads: Mutex::new(vec![]),
            presence: Mutex::new(BTreeMap::new()),
        })
    }
    async fn share(self: &Arc<Self>, epoch: u64, bytes: Vec<u8>) -> Result<()> {
        ensure!(
            bytes.len() <= MAX_DOC,
            "Board exceeds the 4 MiB sharing limit"
        );
        let mut doc = Board::load(&bytes)?;
        let id = doc.id();
        validate(&mut doc, &id)?;
        let auth = match sharing::load_auth(&self.folder, &id, &self.key)? {
            Some(auth) => auth,
            None => {
                let proof = Proof::founder(&self.key);
                ensure!(
                    sharing::previous_authority(&self.folder, &id)?
                        .is_none_or(|authority| authority == proof.root),
                    "Ask the board owner for a new Jotflock invitation"
                );
                Authorization {
                    board: id.clone(),
                    proof,
                    peers: vec![],
                }
            }
        };
        let room = self
            .make_room(epoch, id, auth.proof.root.clone(), doc, Some(auth), None)
            .await;
        self.stop().await;
        *self.room.write().await = Some(room.clone());
        self.persist(&room).await?;
        if self.internet {
            let _ = tokio::time::timeout(Duration::from_secs(5), self.endpoint.online()).await;
        }
        self.invitation().await?;
        Ok(())
    }
    async fn join(
        self: &Arc<Self>,
        epoch: u64,
        ticket: Invitation,
        local: Option<Vec<u8>>,
    ) -> Result<()> {
        ticket.validate(sharing::now())?;
        let id = ticket.body.board.clone();
        let auth = sharing::load_auth(&self.folder, &id, &self.key)?;
        let authority = match &auth {
            Some(auth) => Some(auth.proof.root.clone()),
            None => sharing::previous_authority(&self.folder, &id)?,
        };
        ensure!(
            authority
                .as_ref()
                .is_none_or(|root| root == &ticket.body.root),
            "Invitation belongs to a different board authority"
        );
        ensure!(
            local.is_none() || authority.is_some(),
            "A different local board already has this ID"
        );
        let doc = if let Some(bytes) = local {
            ensure!(bytes.len() <= MAX_DOC, "Board exceeds sharing limit");
            let b = Board::load(&bytes)?;
            ensure!(b.id() == id, "Board identity mismatch");
            b
        } else {
            Board {
                doc: automerge::AutoCommit::new(),
            }
        };
        let room = self
            .make_room(
                epoch,
                id,
                ticket.body.root.clone(),
                doc,
                auth,
                Some(ticket.clone()),
            )
            .await;
        for peer in ticket.body.peers {
            room.peers.write().await.insert(peer.addr.id, peer);
        }
        self.stop().await;
        *self.room.write().await = Some(room.clone());
        self.status(&room, "Waiting for an online participant…");
        self.maintain().await;
        Ok(())
    }
    async fn own_peer(&self, room: &Room) -> Result<Peer> {
        let proof = room
            .proof
            .read()
            .await
            .clone()
            .context("Admission is still pending")?;
        Ok(Peer {
            addr: self.endpoint.addr(),
            proof,
        })
    }
    async fn roster(&self, room: &Room) -> Result<Vec<Peer>> {
        let mut peers = vec![self.own_peer(room).await?];
        peers.extend(
            room.peers
                .read()
                .await
                .values()
                .filter(|p| p.addr.id != self.endpoint.id())
                .take(MAX_PEERS - 1)
                .cloned(),
        );
        Ok(peers)
    }
    async fn persist(&self, room: &Room) -> Result<()> {
        if let Some(proof) = room.proof.read().await.clone() {
            sharing::save_auth(
                &self.folder,
                &Authorization {
                    board: room.id.clone(),
                    proof,
                    peers: room
                        .peers
                        .read()
                        .await
                        .values()
                        .take(MAX_PEERS)
                        .cloned()
                        .collect(),
                },
            )?;
        }
        Ok(())
    }
    async fn learn(&self, room: &Room, peers: Vec<Peer>) -> Result<()> {
        ensure!(peers.len() <= MAX_PEERS, "Too many participants");
        let mut known = room.peers.write().await;
        let mut changed = false;
        for peer in peers {
            peer.validate(&room.id, &room.root)?;
            if peer.addr.id == self.endpoint.id() {
                continue;
            }
            if known.contains_key(&peer.addr.id) || known.len() < MAX_PEERS - 1 {
                changed |= known.get(&peer.addr.id).is_none_or(|p| p.addr != peer.addr);
                known.insert(peer.addr.id, peer);
            }
        }
        drop(known);
        if changed {
            self.persist(room).await?;
        }
        Ok(())
    }
    async fn invitation(&self) -> Result<String> {
        let room = self
            .room
            .read()
            .await
            .clone()
            .context("Open a shared board first")?;
        let proof = room
            .proof
            .read()
            .await
            .clone()
            .context("Waiting for admission")?;
        let invite = Invitation::create(
            &self.key,
            &room.id,
            proof,
            self.roster(&room).await?,
            sharing::now(),
        )?;
        let link = invite.link()?;
        let _ = self
            .events
            .try_send(Event::Invitation(room.epoch, link.clone()));
        Ok(link)
    }
    async fn publish(&self, bytes: Vec<u8>) -> Result<()> {
        ensure!(bytes.len() <= MAX_DOC, "Board exceeds sharing limit");
        if let Some(room) = self.room.read().await.clone() {
            let other = Board::load(&bytes)?;
            ensure!(other.id() == room.id, "Board identity mismatch");
            let mut doc = room.doc.lock().await;
            let mut candidate = Board {
                doc: doc.doc.clone(),
            };
            candidate.doc.merge(&mut other.doc.clone())?;
            validate(&mut candidate, &room.id)?;
            *doc = candidate;
        }
        Ok(())
    }
    async fn maintain(self: &Arc<Self>) {
        let Some(room) = self.room.read().await.clone() else {
            return;
        };
        let peers: Vec<_> = room.peers.read().await.values().cloned().collect();
        let pending = room.proof.read().await.is_none();
        for peer in peers {
            if peer.addr.id == self.endpoint.id() {
                continue;
            }
            if room.connections.lock().await.contains_key(&peer.addr.id) {
                continue;
            }
            let mut dialing = room.dialing.lock().await;
            if !dialing.insert(peer.addr.id) {
                continue;
            }
            drop(dialing);
            let hub = self.clone();
            let r = room.clone();
            tokio::spawn(async move {
                let id = peer.addr.id;
                let result = hub.outgoing(r.clone(), peer).await;
                if let Err(e) = result {
                    if r.alive.load(Ordering::Acquire) {
                        hub.status(&r, format!("Waiting / reconnecting: {e}"));
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
                r.dialing.lock().await.remove(&id);
            });
        }
        let mut doc = room.doc.lock().await;
        if doc.id() == room.id && doc.validate().is_ok() {
            let heads = doc.doc.get_heads();
            let mut last = room.last_heads.lock().await;
            if heads != *last {
                let bytes = doc.save();
                if self
                    .events
                    .try_send(Event::Document(room.epoch, bytes))
                    .is_ok()
                {
                    *last = heads;
                }
            }
        }
        drop(doc);
        let mut presence = room.presence.lock().await;
        presence.retain(|_, (_, time)| time.elapsed() < Duration::from_secs(3));
        let _ = self.events.try_send(Event::Presence(
            room.epoch,
            presence
                .iter()
                .map(|(id, (p, _))| (id.to_string(), p.clone()))
                .collect(),
        ));
        drop(presence);
        let connections = room.connections.lock().await;
        let count = connections.len();
        let relay = connections.values().any(|c| {
            c.paths()
                .iter()
                .any(|p| p.is_selected() && p.remote_addr().is_relay())
        });
        let direct = connections.values().any(|c| {
            c.paths()
                .iter()
                .any(|p| p.is_selected() && p.remote_addr().is_ip())
        });
        let route = if relay && direct {
            "direct + relay"
        } else if relay {
            "relay"
        } else {
            "direct"
        };
        drop(connections);
        if count > 0 {
            self.status(
                &room,
                format!("Encrypted session · {count} editor(s) · {route}"),
            );
        } else if !pending {
            self.status(&room, "Sharing enabled · waiting for participants");
        }
    }
    async fn accept(self: Arc<Self>) {
        while let Some(incoming) = self.endpoint.accept().await {
            let Ok(permit) = self.slots.clone().try_acquire_owned() else {
                incoming.refuse();
                continue;
            };
            let hub = self.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let result = async {
                    let conn = tokio::time::timeout(Duration::from_secs(10), incoming).await??;
                    let Some(room) = hub.room.read().await.clone() else {
                        conn.close(0_u32.into(), b"no session");
                        return Ok(());
                    };
                    let r = room.clone();
                    let result = hub.incoming(room, &conn).await;
                    conn.close(0_u32.into(), b"closed");
                    remove_connection(&r, &conn).await;
                    result
                }
                .await;
                let _ = result as Result<()>;
            });
        }
    }
    async fn register(&self, room: &Room, connection: &Connection) -> Result<()> {
        let mut map = room.connections.lock().await;
        let id = connection.remote_id();
        if let Some(old) = map.get(&id) {
            let preferred = if self.endpoint.id() < id {
                iroh::endpoint::Side::Client
            } else {
                iroh::endpoint::Side::Server
            };
            ensure!(
                connection.side() == preferred && old.side() != preferred,
                "Already connected"
            );
            old.close(0_u32.into(), b"duplicate connection");
        } else {
            ensure!(
                map.len() < MAX_PEERS - 1,
                "Session participant limit reached"
            );
        }
        map.insert(id, connection.clone());
        Ok(())
    }
    async fn incoming(&self, room: Arc<Room>, conn: &Connection) -> Result<()> {
        let (mut tx, mut rx) =
            tokio::time::timeout(Duration::from_secs(10), conn.accept_bi()).await??;
        let outcome: Result<_> = async {
            let Wire::Hello {
                version,
                board,
                root,
                addr,
                credential,
            } = receive(&mut rx).await?
            else {
                return Err(anyhow!("Expected session greeting"));
            };
            ensure!(
                version == 1 && board == room.id && root == room.root,
                "Wrong board or protocol"
            );
            ensure!(
                room.alive.load(Ordering::Acquire),
                "Session has been closed"
            );
            ensure!(
                addr.id == conn.remote_id() && addr.addrs.len() <= 16,
                "Peer identity mismatch"
            );
            room.doc.lock().await.validate()?;
            let own = room
                .proof
                .read()
                .await
                .clone()
                .context("Admission is pending")?;
            let proof = match credential {
                Credential::Member(proof) => {
                    ensure!(
                        proof.member(&board, &root)? == conn.remote_id().to_string(),
                        "Membership belongs to another device"
                    );
                    proof
                }
                Credential::Invite(invite) => {
                    invite.validate(sharing::now())?;
                    ensure!(
                        invite.body.board == board && invite.body.root == root,
                        "Invitation belongs to another board"
                    );
                    own.admit(&self.key, &board, conn.remote_id())?
                }
            };
            self.learn(
                &room,
                vec![Peer {
                    addr,
                    proof: proof.clone(),
                }],
            )
            .await?;
            self.persist(&room).await?;
            Ok(proof)
        }
        .await;
        let proof = match outcome {
            Ok(p) => p,
            Err(e) => {
                let _ = send(&mut tx, &Wire::Reject(e.to_string())).await;
                let _ = tx.finish();
                let _ = tokio::time::timeout(Duration::from_secs(1), tx.stopped()).await;
                return Err(e);
            }
        };
        send(
            &mut tx,
            &Wire::Welcome {
                admitted: proof,
                server: self.own_peer(&room).await?,
                peers: self.roster(&room).await?,
            },
        )
        .await?;
        self.register(&room, conn).await?;
        let mut state = State::new();
        let mut rounds = 0_u64;
        while room.alive.load(Ordering::Acquire) {
            let packet = receive(&mut rx).await?;
            self.apply(&room, conn.remote_id(), &mut state, packet)
                .await?;
            let response = self
                .packet(&room, &mut state, rounds.is_multiple_of(10))
                .await?;
            tokio::time::timeout(Duration::from_secs(20), send(&mut tx, &response)).await??;
            rounds += 1;
        }
        Ok(())
    }
    async fn outgoing(self: &Arc<Self>, room: Arc<Room>, peer: Peer) -> Result<()> {
        peer.validate(&room.id, &room.root)?;
        let conn = tokio::time::timeout(
            Duration::from_secs(8),
            self.endpoint.connect(peer.addr, ALPN),
        )
        .await
        .context("No reachable participant")??;
        let remote = conn.remote_id();
        let result = async {
            ensure!(
                room.alive.load(Ordering::Acquire),
                "Session has been closed"
            );
            let (mut tx, mut rx) = conn.open_bi().await?;
            let credential = if let Some(proof) = room.proof.read().await.clone() {
                Credential::Member(proof)
            } else {
                Credential::Invite(
                    room.ticket
                        .read()
                        .await
                        .clone()
                        .context("Invitation unavailable")?,
                )
            };
            send(
                &mut tx,
                &Wire::Hello {
                    version: 1,
                    board: room.id.clone(),
                    root: room.root.clone(),
                    addr: self.endpoint.addr(),
                    credential,
                },
            )
            .await?;
            match receive(&mut rx).await? {
                Wire::Welcome {
                    admitted,
                    server,
                    peers,
                } => {
                    server.validate(&room.id, &room.root)?;
                    ensure!(server.addr.id == remote, "Server identity mismatch");
                    ensure!(
                        admitted.member(&room.id, &room.root)? == self.endpoint.id().to_string(),
                        "Admission belongs to another device"
                    );
                    *room.proof.write().await = Some(admitted);
                    *room.ticket.write().await = None;
                    self.learn(&room, peers).await?;
                    self.learn(&room, vec![server]).await?;
                    self.persist(&room).await?;
                }
                Wire::Reject(reason) => return Err(anyhow!(reason)),
                _ => return Err(anyhow!("Invalid admission response")),
            }
            self.register(&room, &conn).await?;
            let mut state = State::new();
            let mut rounds = 0_u64;
            while room.alive.load(Ordering::Acquire) {
                let request = self
                    .packet(&room, &mut state, rounds.is_multiple_of(10))
                    .await?;
                tokio::time::timeout(Duration::from_secs(20), send(&mut tx, &request)).await??;
                let response = receive(&mut rx).await?;
                self.apply(&room, remote, &mut state, response).await?;
                rounds += 1;
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
            Ok(())
        }
        .await;
        conn.close(0_u32.into(), b"closed");
        remove_connection(&room, &conn).await;
        result
    }
    async fn packet(&self, room: &Room, state: &mut State, roster: bool) -> Result<Wire> {
        let message = room
            .doc
            .lock()
            .await
            .doc
            .sync()
            .generate_sync_message(state)
            .map(|m| B64.encode(m.encode()));
        Ok(Wire::Data {
            message,
            presence: self.presence.read().await.clone(),
            peers: if roster {
                self.roster(room).await?
            } else {
                vec![]
            },
        })
    }
    async fn apply(
        &self,
        room: &Room,
        remote: EndpointId,
        state: &mut State,
        packet: Wire,
    ) -> Result<()> {
        let Wire::Data {
            message,
            peers,
            presence,
        } = packet
        else {
            return Err(anyhow!("Invalid synchronization packet"));
        };
        presence.validate()?;
        room.presence
            .lock()
            .await
            .insert(remote, (presence, std::time::Instant::now()));
        self.learn(room, peers).await?;
        if let Some(message) = message {
            let bytes = B64.decode(message)?;
            ensure!(bytes.len() <= MAX_FRAME, "Sync message exceeds limit");
            let message = Message::decode(&bytes)?;
            let mut doc = room.doc.lock().await;
            let mut candidate = Board {
                doc: doc.doc.clone(),
            };
            candidate.doc.sync().receive_sync_message(state, message)?;
            ensure!(
                candidate.cards().len() <= 10_000 && candidate.save().len() <= MAX_DOC,
                "Board exceeds collaboration limits"
            );
            if !candidate.id().is_empty() {
                validate(&mut candidate, &room.id)?;
            }
            *doc = candidate;
        }
        Ok(())
    }
}
fn validate(board: &mut Board, id: &str) -> Result<()> {
    board.validate()?;
    ensure!(
        board
            .cards()
            .iter()
            .all(|c| c.x.abs() <= 1_000_000. && c.y.abs() <= 1_000_000.),
        "Object coordinates exceed the sharing range"
    );
    ensure!(board.id() == id, "Cross-board synchronization rejected");
    for (v, _) in board.doc.get_all(ROOT, "id")? {
        ensure!(v.as_str() == Some(id), "Conflicting board identity");
    }
    ensure!(
        board.cards().len() <= 10_000 && board.save().len() <= MAX_DOC,
        "Board exceeds collaboration limits"
    );
    Ok(())
}

async fn remove_connection(room: &Room, connection: &Connection) {
    let mut map = room.connections.lock().await;
    if map
        .get(&connection.remote_id())
        .is_some_and(|c| c.stable_id() == connection.stable_id())
    {
        map.remove(&connection.remote_id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Kind;
    struct Node {
        hub: Arc<Hub>,
        _dir: tempfile::TempDir,
        _events: Receiver<Event>,
        accept: tokio::task::JoinHandle<()>,
        tick: tokio::task::JoinHandle<()>,
    }
    impl Node {
        async fn new(local: bool, relay: bool) -> Result<Self> {
            let dir = tempfile::tempdir()?;
            let (tx, events) = mpsc::sync_channel(256);
            let hub = Hub::bind(dir.path().into(), tx, local, relay).await?;
            let h = hub.clone();
            let accept = tokio::spawn(async move {
                h.accept().await;
            });
            let h = hub.clone();
            let tick = tokio::spawn(async move {
                loop {
                    h.maintain().await;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            });
            Ok(Self {
                hub,
                _dir: dir,
                _events: events,
                accept,
                tick,
            })
        }
        async fn close(&self) {
            self.tick.abort();
            self.hub.stop().await;
            self.accept.abort();
            self.hub.endpoint.close().await;
        }
        async fn room(&self) -> Arc<Room> {
            self.hub.room.read().await.clone().unwrap()
        }
        async fn wait_cards(&self, count: usize) -> Result<()> {
            tokio::time::timeout(Duration::from_secs(25), async {
                loop {
                    let room = self.room().await;
                    if room.doc.lock().await.cards().len() == count {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
            .await
            .context("Board did not converge")?;
            Ok(())
        }
        async fn add(&self, text: &str) -> Result<()> {
            let room = self.room().await;
            let bytes = room.doc.lock().await.save();
            let mut doc = Board::load(&bytes)?;
            doc.add(Kind::Sticky, text, 50., 80.)?;
            self.hub.publish(doc.save()).await
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn two_editors_can_join_with_the_same_invitation() -> Result<()> {
        let host = Node::new(true, false).await?;
        let first = Node::new(true, false).await?;
        let second = Node::new(true, false).await?;
        let mut board = Board::new()?;
        board.add(Kind::Sticky, "Shared note", 0., 0.)?;
        let board_id = board.id();
        let former_access = Authorization {
            board: board_id.clone(),
            proof: Proof::founder(&host.hub.key),
            peers: vec![],
        };
        let access_path = first
            .hub
            .folder
            .join("network")
            .join(format!("{board_id}.json"));
        sharing::private_write(&access_path, &serde_json::to_vec(&former_access)?)?;
        let mut offline_copy = Board::load(&board.save())?;
        offline_copy.add(Kind::Sticky, "Offline edit", 0., 0.)?;
        assert!(first.hub.share(1, offline_copy.save()).await.is_err());
        host.hub.share(1, board.save()).await?;
        let link = host.hub.invitation().await?;
        for (guest, local) in [(&first, Some(offline_copy.save())), (&second, None)] {
            guest
                .hub
                .join(1, Invitation::parse(&link, sharing::now())?, local)
                .await?;
            guest.wait_cards(2).await?;
        }
        first.add("First editor").await?;
        second.add("Second editor").await?;
        host.wait_cards(4).await?;
        first.wait_cards(4).await?;
        second.wait_cards(4).await?;
        host.close().await;
        first.close().await;
        second.close().await;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn three_devices_delegate_continue_without_founder_and_reconnect() -> Result<()> {
        let a = Node::new(true, false).await?;
        let b = Node::new(true, false).await?;
        let c = Node::new(true, false).await?;
        let mut board = Board::new()?;
        board.add(Kind::Sticky, "Original", 0., 0.)?;
        let id = board.id();
        a.hub.share(1, board.save()).await?;
        let link = a.hub.invitation().await?;
        b.hub
            .join(1, Invitation::parse(&link, sharing::now())?, None)
            .await?;
        b.wait_cards(1).await?;
        b.add("From editor B").await?;
        a.wait_cards(2).await?;
        a.close().await;
        let delegated = b.hub.invitation().await?;
        c.hub
            .join(1, Invitation::parse(&delegated, sharing::now())?, None)
            .await?;
        c.wait_cards(2).await?;
        *c.hub.presence.write().await = Presence {
            name: "Editor C".into(),
            cursor: Some([100., 200.]),
            selected: None,
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if b.room()
                    .await
                    .presence
                    .lock()
                    .await
                    .get(&c.hub.endpoint.id())
                    .is_some_and(|(p, _)| p.name == "Editor C" && p.cursor == Some([100., 200.]))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await?;
        c.add("From editor C").await?;
        b.wait_cards(3).await?;
        let room = b.room().await;
        let cached = room.doc.lock().await.save();
        // Stop and re-offer the same board using saved membership, without a link.
        b.hub.stop().await;
        c.add("While B is offline").await?;
        b.hub.share(2, cached).await?;
        b.wait_cards(4).await?;
        assert_eq!(b.room().await.id, id);
        let root = b.room().await.root.clone();
        assert_eq!(
            c.room()
                .await
                .proof
                .read()
                .await
                .as_ref()
                .unwrap()
                .member(&id, &root)?,
            c.hub.endpoint.id().to_string()
        );
        b.close().await;
        c.close().await;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn unauthorized_device_receives_rejection_before_document_data() -> Result<()> {
        let a = Node::new(true, false).await?;
        let outsider = Node::new(true, false).await?;
        let mut doc = Board::new()?;
        let id = doc.id();
        a.hub.share(1, doc.save()).await?;
        let conn = outsider
            .hub
            .endpoint
            .connect(a.hub.endpoint.addr(), ALPN)
            .await?;
        let (mut tx, mut rx) = conn.open_bi().await?;
        send(
            &mut tx,
            &Wire::Hello {
                version: 1,
                board: id,
                root: a.hub.key.public().to_string(),
                addr: outsider.hub.endpoint.addr(),
                credential: Credential::Member(Proof::founder(&outsider.hub.key)),
            },
        )
        .await?;
        assert!(matches!(receive(&mut rx).await?, Wire::Reject(_)));
        assert!(a.room().await.connections.lock().await.is_empty());
        conn.close(0_u32.into(), b"test complete");
        a.close().await;
        outsider.close().await;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn saved_membership_reconnects_on_a_new_endpoint_after_restart() -> Result<()> {
        let a = Node::new(true, false).await?;
        let b = Node::new(true, false).await?;
        let mut doc = Board::new()?;
        doc.add(Kind::Sticky, "Original", 0., 0.)?;
        a.hub.share(1, doc.save()).await?;
        let ticket = Invitation::parse(&a.hub.invitation().await?, sharing::now())?;
        b.hub.join(1, ticket, None).await?;
        b.wait_cards(1).await?;
        let cached = b.room().await.doc.lock().await.save();
        let identity = b.hub.endpoint.id();
        b.close().await;
        a.add("Offline update").await?;
        let (tx, _events) = mpsc::sync_channel(256);
        let restarted = Hub::bind(b._dir.path().into(), tx, true, false).await?;
        assert_eq!(identity, restarted.endpoint.id());
        let h = restarted.clone();
        let accept = tokio::spawn(async move {
            h.accept().await;
        });
        restarted.share(2, cached).await?;
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                restarted.maintain().await;
                if restarted
                    .room
                    .read()
                    .await
                    .as_ref()
                    .unwrap()
                    .doc
                    .lock()
                    .await
                    .cards()
                    .len()
                    == 2
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
        restarted.stop().await;
        accept.abort();
        restarted.endpoint.close().await;
        a.close().await;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stopped_session_rejects_an_in_progress_admission() -> Result<()> {
        let a = Node::new(true, false).await?;
        let b = Node::new(true, false).await?;
        let mut doc = Board::new()?;
        let board = doc.id();
        a.hub.share(1, doc.save()).await?;
        let ticket = Invitation::parse(&a.hub.invitation().await?, sharing::now())?;
        let conn = b.hub.endpoint.connect(a.hub.endpoint.addr(), ALPN).await?;
        let (mut tx, mut rx) = conn.open_bi().await?;
        let bytes = serde_json::to_vec(&Wire::Hello {
            version: 1,
            board,
            root: a.hub.key.public().to_string(),
            addr: b.hub.endpoint.addr(),
            credential: Credential::Invite(ticket),
        })?;
        tx.write_all(&(bytes.len() as u32).to_be_bytes()).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        a.hub.stop().await;
        tx.write_all(&bytes).await?;
        assert!(matches!(receive(&mut rx).await?, Wire::Reject(_)));
        assert!(sharing::load_auth(&b.hub.folder, &doc.id(), &b.hub.key)?.is_none());
        conn.close(0_u32.into(), b"complete");
        a.close().await;
        b.close().await;
        Ok(())
    }
    #[test]
    fn invalid_presence_is_rejected() {
        assert!(Presence {
            name: "x".repeat(65),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Presence {
            cursor: Some([f32::NAN, 0.]),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(Presence {
            name: "Editor".into(),
            cursor: Some([100., 200.]),
            selected: None
        }
        .validate()
        .is_ok());
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn stale_presence_expires_without_document_changes() -> Result<()> {
        let node = Node::new(true, false).await?;
        let mut doc = Board::new()?;
        node.hub.share(1, doc.save()).await?;
        let room = node.room().await;
        room.presence.lock().await.insert(
            SecretKey::generate().public(),
            (
                Presence::default(),
                std::time::Instant::now() - Duration::from_secs(4),
            ),
        );
        node.hub.maintain().await;
        assert!(room.presence.lock().await.is_empty());
        assert!(room.doc.lock().await.cards().is_empty());
        node.close().await;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires public internet relay infrastructure"]
    async fn public_relay_only_synchronizes_encrypted_board() -> Result<()> {
        let a = Node::new(false, true).await?;
        let b = Node::new(false, true).await?;
        tokio::time::timeout(Duration::from_secs(30), a.hub.endpoint.online()).await?;
        tokio::time::timeout(Duration::from_secs(30), b.hub.endpoint.online()).await?;
        let mut doc = Board::new()?;
        doc.add(Kind::Sticky, "Over encrypted relay", 0., 0.)?;
        a.hub.share(1, doc.save()).await?;
        let ticket = Invitation::parse(&a.hub.invitation().await?, sharing::now())?;
        b.hub.join(1, ticket, None).await?;
        b.wait_cards(1).await?;
        let room = b.room().await;
        let connections = room.connections.lock().await;
        assert!(!connections.is_empty());
        assert!(connections.values().all(|c| c
            .paths()
            .iter()
            .any(|p| p.is_selected() && p.remote_addr().is_relay())));
        drop(connections);
        b.add("Relay edit").await?;
        a.wait_cards(2).await?;
        a.close().await;
        b.close().await;
        Ok(())
    }
}
