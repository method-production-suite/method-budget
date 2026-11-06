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
//!   [`Relay`], which forwards encrypted packets;
//! - a connection from a key not on the allow list is closed before any
//!   frame is read.

use std::collections::{BTreeMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use iroh::endpoint::{presets, Connection, PortmapperConfig, RecvStream, SendStream, VarInt};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, RelayUrl, SecretKey};
use serde::{Deserialize, Serialize};

/// The protocol's name in the connection handshake. A later, incompatible
/// protocol takes a new number.
pub const ALPN: &[u8] = b"method-budget/sync/1";
/// The largest frame either side will read or write.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;
/// How long the two sides have to say hello.
const HELLO_WAIT: Duration = Duration::from_secs(10);
/// The close code for a key that isn't on the allow list.
const CLOSE_NOT_ALLOWED: u32 = 1;

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

/// This computer's endpoint. Dropping it closes every link.
#[derive(Debug, Clone)]
pub struct Node {
    endpoint: Endpoint,
    allowed: Arc<Mutex<HashSet<EndpointId>>>,
}

impl Node {
    /// Starts the endpoint with its 32-byte secret key. This is the first
    /// thing that touches the network.
    pub async fn start(secret: [u8; 32], relay: Relay) -> Result<Node, SyncError> {
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&secret))
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(relay.mode())
            .portmapper_config(PortmapperConfig::Disabled)
            .bind()
            .await
            .map_err(|e| fail("couldn't start the live link", e))?;
        Ok(Node {
            endpoint,
            allowed: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    /// Starts an endpoint on this computer's loopback address, with no relay:
    /// nothing leaves the computer. For tests.
    pub async fn start_loopback(secret: [u8; 32]) -> Result<Node, SyncError> {
        let endpoint = Endpoint::builder(presets::Minimal)
            .secret_key(SecretKey::from_bytes(&secret))
            .alpns(vec![ALPN.to_vec()])
            .relay_mode(RelayMode::Disabled)
            .portmapper_config(PortmapperConfig::Disabled)
            .clear_ip_transports()
            .bind_addr(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
            .map_err(|e| fail("couldn't use the loopback address", e))?
            .bind()
            .await
            .map_err(|e| fail("couldn't start the live link", e))?;
        Ok(Node {
            endpoint,
            allowed: Arc::new(Mutex::new(HashSet::new())),
        })
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

    /// Waits for the next member to connect. A key not on the allow list is
    /// closed here and waiting goes on. `None` once the node is closed.
    pub async fn accept(&self) -> Option<Link> {
        loop {
            let incoming = self.endpoint.accept().await?;
            let Ok(accepting) = incoming.accept() else {
                continue;
            };
            let Ok(conn) = accepting.await else { continue };
            if !self.is_allowed(&conn.remote_id()) {
                conn.close(VarInt::from_u32(CLOSE_NOT_ALLOWED), b"not a member");
                continue;
            }
            let Ok((send, recv)) = conn.accept_bi().await else {
                continue;
            };
            return Some(Link { conn, send, recv });
        }
    }

    /// Closes every link and the endpoint: this person turned live mode off,
    /// or the budget closed.
    pub async fn close(&self) {
        self.endpoint.close().await;
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
        let theirs = tokio::time::timeout(HELLO_WAIT, self.recv())
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
