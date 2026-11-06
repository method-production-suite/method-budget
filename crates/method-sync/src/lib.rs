//! The live link between members of a shared budget (roadmap 3.18;
//! docs/research/collaboration.md, section 6; decisions.md, entry 98): an
//! encrypted connection between two computers that know each other's keys,
//! carrying length-prefixed frames. It knows nothing about budgets or logs:
//! the changes it carries are signed by their authors, and whoever receives
//! them checks them as if they had come through the folder.
//!
//! What this crate does on the network is the point of it, so:
//!
//! - nothing happens until [`Node::start`] is called, and the app calls that
//!   only after this person has turned live mode on for themselves;
//! - the endpoint is built from iroh's `Minimal` preset, so its address is
//!   published nowhere; members find each other through the shared folder;
//! - the router is left alone: no UPnP, PCP or NAT-PMP, and the crate that
//!   speaks them isn't built;
//! - the only outside machine it may talk to is the relay named in
//!   [`Relay`], which forwards encrypted packets, and a relay's name is looked
//!   up through this computer's own DNS servers, never public ones;
//! - a connection from a key not on the allow list is closed before any
//!   frame is read.

use std::collections::{BTreeMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iroh::dns::DnsResolver;
use iroh::endpoint::{
    presets, Builder, Connection, Incoming, PortmapperConfig, RecvStream, SendStream, VarInt,
};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;

/// The protocol's name in the connection handshake. A later, incompatible
/// protocol takes a new number.
pub const ALPN: &[u8] = b"method-budget/sync/1";
/// The largest frame either side will read or write.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
/// How long the two sides have to say hello.
const HELLO_WAIT: Duration = Duration::from_secs(10);
/// The close code for a key that isn't on the allow list.
const CLOSE_NOT_ALLOWED: u32 = 1;
/// The close code for a connection that opened no stream in time.
const CLOSE_TOO_SLOW: u32 = 2;
/// How long a connection has for its handshake, and then to open its stream.
const ADMIT_WAIT: Duration = Duration::from_secs(10);
/// How many connections may be on their way in at once, and wait to be taken.
const PENDING_MAX: usize = 32;

/// Which relay carries a connection two computers can't make directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Relay {
    /// None: direct connections only.
    None,
    /// The free public relays run by iroh's makers. They see who talks to
    /// whom and their IP addresses, never the budget.
    Public,
    /// A relay this production runs itself.
    Own(RelayUrl),
}

impl Relay {
    /// A relay of the production's own from what the person typed.
    pub fn own(url: &str) -> Result<Relay, SyncError> {
        RelayUrl::from_str(url.trim())
            .map(Relay::Own)
            .map_err(|e| SyncError::new(format!("not a relay address: {e}")))
    }

    fn mode(&self) -> RelayMode {
        match self {
            Relay::None => RelayMode::Disabled,
            Relay::Public => RelayMode::Default,
            Relay::Own(url) => RelayMode::Custom(url.clone().into()),
        }
    }
}

/// Anything that went wrong on the link, in words for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncError(pub String);

impl SyncError {
    fn new(text: impl Into<String>) -> SyncError {
        SyncError(text.into())
    }
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SyncError {}

fn fail(what: &str, e: impl std::fmt::Display) -> SyncError {
    SyncError::new(format!("{what}: {e}"))
}

/// What two computers say to each other. Unknown frames fail to read: a
/// change to this list takes a new [`ALPN`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "frame", rename_all = "snake_case")]
pub enum Frame {
    /// The first frame each way: which budget this link is for.
    Hello { budget: String },
    /// The highest change number this computer holds for each person.
    Have { frontier: BTreeMap<String, u64> },
    /// Changes the other side lacks, as the log's own signed lines.
    Changes { lines: Vec<String> },
}

/// This computer's endpoint. [`Node::close`] closes it and every link.
#[derive(Debug, Clone)]
pub struct Node {
    endpoint: Endpoint,
    allowed: Arc<Mutex<HashSet<EndpointId>>>,
    arrived: Arc<tokio::sync::Mutex<mpsc::Receiver<Link>>>,
}

/// The settings every endpoint shares, whatever its relay. iroh's default
/// resolver falls back to public DNS servers when the system's fail, which
/// would tell them a relay's name, so it is told not to.
fn builder(secret: [u8; 32], relay: &Relay) -> Builder {
    Endpoint::builder(presets::Minimal)
        .secret_key(SecretKey::from_bytes(&secret))
        .alpns(vec![ALPN.to_vec()])
        .relay_mode(relay.mode())
        .portmapper_config(PortmapperConfig::Disabled)
        .dns_resolver(
            DnsResolver::builder()
                .with_system_defaults()
                .disable_fallback()
                .build(),
        )
}

impl Node {
    /// Starts the endpoint with its 32-byte secret key. This is the first
    /// thing that touches the network.
    pub async fn start(secret: [u8; 32], relay: Relay) -> Result<Node, SyncError> {
        let endpoint = builder(secret, &relay)
            .bind()
            .await
            .map_err(|e| fail("couldn't start the live link", e))?;
        Ok(Node::around(endpoint))
    }

    /// Starts an endpoint on this computer's loopback address, with no relay:
    /// nothing leaves the computer. For tests.
    pub async fn start_loopback(secret: [u8; 32]) -> Result<Node, SyncError> {
        let endpoint = builder(secret, &Relay::None)
            .clear_ip_transports()
            .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|e| fail("couldn't use the loopback address", e))?
            .bind()
            .await
            .map_err(|e| fail("couldn't start the live link", e))?;
        Ok(Node::around(endpoint))
    }

    fn around(endpoint: Endpoint) -> Node {
        let allowed = Arc::new(Mutex::new(HashSet::new()));
        let (arrive, arrived) = mpsc::channel(PENDING_MAX);
        tokio::spawn(accept_loop(endpoint.clone(), allowed.clone(), arrive));
        Node {
            endpoint,
            allowed,
            arrived: Arc::new(tokio::sync::Mutex::new(arrived)),
        }
    }

    /// This computer's key, as members' files name it.
    pub fn id(&self) -> String {
        self.endpoint.id().to_string()
    }

    /// How to reach this computer, for its heartbeat file: its key and its
    /// addresses as they stand. Text the other side gives to [`Node::connect`].
    pub fn address(&self) -> Result<String, SyncError> {
        serde_json::to_string(&self.endpoint.addr())
            .map_err(|e| fail("couldn't write the address", e))
    }

    /// Whether anything outside this crate has been told where this computer
    /// is. Always false: see the module's notes.
    pub fn publishes_address(&self) -> bool {
        self.endpoint
            .address_lookup()
            .map(|services| !services.is_empty())
            .unwrap_or(false)
    }

    /// Replaces the keys allowed to connect (the budget's members, from the
    /// shared folder). A link already open is not closed by this.
    pub fn allow(&self, ids: &[String]) -> Result<(), SyncError> {
        let mut set = HashSet::with_capacity(ids.len());
        for id in ids {
            set.insert(EndpointId::from_str(id).map_err(|e| fail("not a member's key", e))?);
        }
        *self.allowed.lock().unwrap_or_else(|p| p.into_inner()) = set;
        Ok(())
    }

    fn is_allowed(&self, id: &EndpointId) -> bool {
        self.allowed
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains(id)
    }

    /// Connects to a member from the text of their [`Node::address`]. Their
    /// key must be on the allow list too.
    pub async fn connect(&self, address: &str) -> Result<Link, SyncError> {
        let addr: EndpointAddr =
            serde_json::from_str(address).map_err(|e| fail("not an address", e))?;
        if !self.is_allowed(&addr.id) {
            return Err(SyncError::new(
                "that computer isn't a member of this budget",
            ));
        }
        let conn = self
            .endpoint
            .connect(addr, ALPN)
            .await
            .map_err(|e| fail("couldn't connect", e))?;
        let (send, recv) = conn
            .open_bi()
            .await
            .map_err(|e| fail("couldn't open the link", e))?;
        Ok(Link { conn, send, recv })
    }

    /// Waits for the next member to connect. Strangers never get here (see
    /// [`accept_loop`]). `None` once the node is closed.
    pub async fn accept(&self) -> Option<Link> {
        self.arrived.lock().await.recv().await
    }

    /// Closes every link and the endpoint: this person turned live mode off,
    /// or the budget closed.
    pub async fn close(&self) {
        self.endpoint.close().await;
    }
}

/// Takes connections as they come. Each is worked through on its own task,
/// with a time limit, so a slow handshake or a silent member never keeps
/// another member waiting; past [`PENDING_MAX`] at once, new ones are refused.
async fn accept_loop(
    endpoint: Endpoint,
    allowed: Arc<Mutex<HashSet<EndpointId>>>,
    arrive: mpsc::Sender<Link>,
) {
    let pending = Arc::new(Semaphore::new(PENDING_MAX));
    while let Some(incoming) = endpoint.accept().await {
        let Ok(permit) = pending.clone().try_acquire_owned() else {
            incoming.refuse();
            continue;
        };
        let allowed = allowed.clone();
        let arrive = arrive.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Some(link) = admit(incoming, &allowed).await {
                let _ = arrive.send(link).await;
            }
        });
    }
}

/// A connection, once its handshake is done, its key is on the list and it
/// has opened its stream, each in [`ADMIT_WAIT`].
async fn admit(incoming: Incoming, allowed: &Mutex<HashSet<EndpointId>>) -> Option<Link> {
    let accepting = incoming.accept().ok()?;
    let conn = timeout(ADMIT_WAIT, accepting).await.ok()?.ok()?;
    let on_list = allowed
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .contains(&conn.remote_id());
    if !on_list {
        conn.close(VarInt::from_u32(CLOSE_NOT_ALLOWED), b"not a member");
        return None;
    }
    match timeout(ADMIT_WAIT, conn.accept_bi()).await {
        Ok(Ok((send, recv))) => Some(Link { conn, send, recv }),
        _ => {
            conn.close(VarInt::from_u32(CLOSE_TOO_SLOW), b"too slow");
            None
        }
    }
}

/// One open connection to another member.
#[derive(Debug)]
pub struct Link {
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
}

impl Link {
    /// The other computer's key, proved by the connection's handshake.
    pub fn peer(&self) -> String {
        self.conn.remote_id().to_string()
    }

    /// Says which budget this link is for and checks the other side says the
    /// same, so a member of two budgets never mixes them.
    pub async fn hello(&mut self, budget: &str) -> Result<(), SyncError> {
        self.send(&Frame::Hello {
            budget: budget.to_string(),
        })
        .await?;
        let theirs = timeout(HELLO_WAIT, self.recv())
            .await
            .map_err(|_| SyncError::new("the other computer didn't answer"))??;
        match theirs {
            Frame::Hello { budget: b } if b == budget => Ok(()),
            Frame::Hello { .. } => Err(SyncError::new(
                "the other computer has a different budget open",
            )),
            _ => Err(SyncError::new("the other computer didn't say hello first")),
        }
    }

    /// Sends one frame.
    pub async fn send(&mut self, frame: &Frame) -> Result<(), SyncError> {
        let bytes = serde_json::to_vec(frame).map_err(|e| fail("couldn't write a frame", e))?;
        if bytes.len() > MAX_FRAME {
            return Err(SyncError::new("that frame is too large to send"));
        }
        self.send
            .write_all(&(bytes.len() as u32).to_be_bytes())
            .await
            .map_err(|e| fail("couldn't send", e))?;
        self.send
            .write_all(&bytes)
            .await
            .map_err(|e| fail("couldn't send", e))
    }

    /// Waits for the next frame. One larger than [`MAX_FRAME`] is refused
    /// before it is read, and ends the link.
    pub async fn recv(&mut self) -> Result<Frame, SyncError> {
        let mut len = [0u8; 4];
        self.recv
            .read_exact(&mut len)
            .await
            .map_err(|e| fail("the link ended", e))?;
        let len = u32::from_be_bytes(len) as usize;
        if len > MAX_FRAME {
            self.conn.close(VarInt::from_u32(0), b"frame too large");
            return Err(SyncError::new(
                "the other computer sent a frame too large to read",
            ));
        }
        let mut bytes = vec![0u8; len];
        self.recv
            .read_exact(&mut bytes)
            .await
            .map_err(|e| fail("the link ended", e))?;
        serde_json::from_slice(&bytes).map_err(|e| fail("couldn't read a frame", e))
    }

    /// Sends raw bytes as if they were a frame's length, for tests of what a
    /// misbehaving computer can do.
    #[doc(hidden)]
    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<(), SyncError> {
        self.send
            .write_all(bytes)
            .await
            .map_err(|e| fail("couldn't send", e))
    }
}
