//! Links to every member who is live, kept up in the background and driven
//! from ordinary code: [`Mesh::set_peers`] says who should be connected,
//! [`Mesh::send_to`] and [`Mesh::broadcast`] hand over frames, and
//! [`Mesh::events`] says what happened. The app's one-second timer does all
//! three, so nothing about the budget happens on these threads.
//!
//! One mesh serves one budget, with its own endpoint and allow list, so a
//! member of budget A can't reach budget B through it. A peer dropped from
//! [`Mesh::set_peers`] loses its link at once (a person removed from the
//! budget stops receiving changes). Whichever key sorts first dials, and goes
//! on trying every [`RETRY`]; a link that fails comes back the same way.
//! Dropping the mesh closes the endpoint: it is the only way out, so there is
//! no way to leave it online by accident.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{interval, timeout, MissedTickBehavior};

use crate::{Frame, Link, Node, Relay, SyncError};

/// How often a member with no link is tried again.
pub const RETRY: Duration = Duration::from_secs(3);
/// How long a dial and its hello may take.
const DIAL_WAIT: Duration = Duration::from_secs(20);
/// How long closing may wait to tell the others.
const CLOSE_WAIT: Duration = Duration::from_secs(2);
/// Frames waiting to go to one peer. Past this the link is closed; when it
/// comes back the two sides compare what they hold, so nothing is lost.
const SEND_QUEUE: usize = 256;
/// Events waiting for the app's timer. Past this a link is closed, likewise.
const EVENT_QUEUE: usize = 4096;

/// What happened on the links since the last look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A link to this peer is open.
    Up { peer: String },
    /// The link to this peer ended.
    Down { peer: String },
    /// The peer said what it holds.
    Have {
        peer: String,
        frontier: BTreeMap<String, u64>,
    },
    /// The peer sent changes.
    Changes { peer: String, lines: Vec<String> },
}

enum Cmd {
    Peers(BTreeMap<String, String>),
    SendTo(String, Frame),
    Broadcast(Frame),
}

/// The live links of one budget on this computer.
#[derive(Debug)]
pub struct Mesh {
    node: Node,
    cmds: mpsc::UnboundedSender<Cmd>,
    events: std_mpsc::Receiver<Event>,
    connected: Arc<AtomicUsize>,
    runtime: Option<Runtime>,
}

impl Mesh {
    /// Starts the endpoint and the links for `budget`. This is the first
    /// thing that touches the network; call it only once this person has
    /// turned live mode on for themselves.
    pub fn start(secret: [u8; 32], relay: Relay, budget: &str) -> Result<Mesh, SyncError> {
        Mesh::begin(budget, move || Box::pin(Node::start(secret, relay)))
    }

    /// A mesh on this computer's loopback address, with no relay: nothing
    /// leaves the computer. For tests.
    pub fn start_loopback(secret: [u8; 32], budget: &str) -> Result<Mesh, SyncError> {
        Mesh::begin(budget, move || Box::pin(Node::start_loopback(secret)))
    }

    fn begin<F>(budget: &str, start: F) -> Result<Mesh, SyncError>
    where
        F: FnOnce() -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<Node, SyncError>> + Send>,
            > + Send
            + 'static,
    {
        let budget = budget.to_string();
        // On a thread of its own, since a caller may already be inside a
        // runtime, where blocking on another one panics.
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("method-live")
                .enable_all()
                .build()
                .map_err(|e| SyncError(format!("couldn't start the live link: {e}")))?;
            let node = runtime.block_on(start())?;
            let (cmds, cmd_rx) = mpsc::unbounded_channel();
            let (event_tx, events) = std_mpsc::sync_channel(EVENT_QUEUE);
            let connected = Arc::new(AtomicUsize::new(0));
            runtime.spawn(run(
                node.clone(),
                budget,
                cmd_rx,
                event_tx,
                connected.clone(),
            ));
            Ok(Mesh {
                node,
                cmds,
                events,
                connected,
                runtime: Some(runtime),
            })
        })
        .join()
        .map_err(|_| SyncError("couldn't start the live link".to_string()))?
    }

    /// This computer's key on the link.
    pub fn id(&self) -> String {
        self.node.id()
    }

    /// How to reach this computer, for its heartbeat file.
    pub fn address(&self) -> Result<String, SyncError> {
        self.node.address()
    }

    /// Who should be connected: each member's key and address. Anyone else
    /// loses their link at once and can't connect.
    pub fn set_peers(&self, peers: BTreeMap<String, String>) {
        let _ = self.cmds.send(Cmd::Peers(peers));
    }

    /// Sends a frame to one peer, if it has a link.
    pub fn send_to(&self, peer: &str, frame: Frame) {
        let _ = self.cmds.send(Cmd::SendTo(peer.to_string(), frame));
    }

    /// Sends a frame to every peer with a link.
    pub fn broadcast(&self, frame: Frame) {
        let _ = self.cmds.send(Cmd::Broadcast(frame));
    }

    /// What happened since the last call.
    pub fn events(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }

    /// How many links are open.
    pub fn connected(&self) -> usize {
        self.connected.load(Ordering::Relaxed)
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        let Some(runtime) = self.runtime.take() else {
            return;
        };
        let node = self.node.clone();
        // On a thread of its own for the same reason as starting.
        let closing = std::thread::spawn(move || {
            runtime.block_on(async {
                let _ = timeout(CLOSE_WAIT, node.close()).await;
            });
            runtime.shutdown_background();
        });
        let _ = closing.join();
    }
}

struct Open {
    to_send: mpsc::Sender<Frame>,
    tasks: [JoinHandle<()>; 2],
    /// Which link this is, so a late end from an old one is ignored.
    serial: u64,
}

impl Open {
    fn stop(&self) {
        for t in &self.tasks {
            t.abort();
        }
    }
}

async fn run(
    node: Node,
    budget: String,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
    events: std_mpsc::SyncSender<Event>,
    connected: Arc<AtomicUsize>,
) {
    let me = node.id();
    let (link_tx, mut link_rx) = mpsc::channel::<Link>(32);
    let (dialed_tx, mut dialed_rx) = mpsc::unbounded_channel::<String>();
    let (ended_tx, mut ended_rx) = mpsc::unbounded_channel::<(String, u64)>();
    tokio::spawn(accept_all(node.clone(), budget.clone(), link_tx.clone()));

    let mut wanted: BTreeMap<String, String> = BTreeMap::new();
    let mut links: HashMap<String, Open> = HashMap::new();
    let mut dialing: HashSet<String> = HashSet::new();
    let mut serial = 0u64;
    let mut retry = interval(RETRY);
    retry.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            cmd = cmds.recv() => {
                match cmd {
                    None => break,
                    Some(Cmd::Peers(peers)) => {
                        let ids: Vec<String> = peers.keys().cloned().collect();
                        node.allow(&ids);
                        wanted = peers;
                        let gone: Vec<String> =
                            links.keys().filter(|id| !wanted.contains_key(*id)).cloned().collect();
                        for id in gone {
                            if let Some(open) = links.remove(&id) {
                                open.stop();
                                connected.store(links.len(), Ordering::Relaxed);
                                let _ = events.try_send(Event::Down { peer: id });
                            }
                        }
                    }
                    Some(Cmd::SendTo(peer, frame)) => {
                        if let Some(open) = links.get(&peer) {
                            if open.to_send.try_send(frame).is_err() {
                                let _ = ended_tx.send((peer, open.serial));
                            }
                        }
                    }
                    Some(Cmd::Broadcast(frame)) => {
                        for (peer, open) in &links {
                            if open.to_send.try_send(frame.clone()).is_err() {
                                let _ = ended_tx.send((peer.clone(), open.serial));
                            }
                        }
                    }
                }
                dial_missing(&node, &me, &budget, &wanted, &links, &mut dialing, &link_tx, &dialed_tx);
            }
            Some(link) = link_rx.recv() => {
                let peer = link.peer();
                if !wanted.contains_key(&peer) || links.contains_key(&peer) {
                    continue;
                }
                serial += 1;
                links.insert(peer.clone(), open_link(link, serial, events.clone(), ended_tx.clone()));
                connected.store(links.len(), Ordering::Relaxed);
                let _ = events.try_send(Event::Up { peer });
            }
            Some(peer) = dialed_rx.recv() => {
                dialing.remove(&peer);
            }
            Some((peer, which)) = ended_rx.recv() => {
                if links.get(&peer).is_some_and(|open| open.serial == which) {
                    if let Some(open) = links.remove(&peer) {
                        open.stop();
                    }
                    connected.store(links.len(), Ordering::Relaxed);
                    let _ = events.try_send(Event::Down { peer });
                }
            }
            _ = retry.tick() => {
                dial_missing(&node, &me, &budget, &wanted, &links, &mut dialing, &link_tx, &dialed_tx);
            }
        }
    }
    for open in links.values() {
        open.stop();
    }
}

async fn accept_all(node: Node, budget: String, links: mpsc::Sender<Link>) {
    while let Some(mut link) = node.accept().await {
        let budget = budget.clone();
        let links = links.clone();
        tokio::spawn(async move {
            if link.hello(&budget).await.is_ok() {
                let _ = links.send(link).await;
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn dial_missing(
    node: &Node,
    me: &str,
    budget: &str,
    wanted: &BTreeMap<String, String>,
    links: &HashMap<String, Open>,
    dialing: &mut HashSet<String>,
    link_tx: &mpsc::Sender<Link>,
    dialed_tx: &mpsc::UnboundedSender<String>,
) {
    for (peer, address) in wanted {
        if me >= peer.as_str() || links.contains_key(peer) || !dialing.insert(peer.clone()) {
            continue;
        }
        let (node, address, budget, peer) = (
            node.clone(),
            address.clone(),
            budget.to_string(),
            peer.clone(),
        );
        let (link_tx, dialed_tx) = (link_tx.clone(), dialed_tx.clone());
        tokio::spawn(async move {
            let dialed = timeout(DIAL_WAIT, async {
                let mut link = node.connect(&address).await?;
                link.hello(&budget).await?;
                Ok::<Link, SyncError>(link)
            })
            .await;
            if let Ok(Ok(link)) = dialed {
                let _ = link_tx.send(link).await;
            }
            let _ = dialed_tx.send(peer);
        });
    }
}

/// Starts a link's two directions. Reading and writing are separate tasks
/// because a read can't be interrupted part way through a frame.
fn open_link(
    link: Link,
    serial: u64,
    events: std_mpsc::SyncSender<Event>,
    ended: mpsc::UnboundedSender<(String, u64)>,
) -> Open {
    let peer = link.peer();
    let (mut sender, mut receiver) = link.split();
    let (to_send, mut queue) = mpsc::channel::<Frame>(SEND_QUEUE);

    let (who, done) = (peer.clone(), ended.clone());
    let writer = tokio::spawn(async move {
        while let Some(frame) = queue.recv().await {
            if sender.send(&frame).await.is_err() {
                break;
            }
        }
        let _ = done.send((who, serial));
    });
    let reader = tokio::spawn(async move {
        loop {
            let event = match receiver.recv().await {
                Ok(Frame::Have { frontier }) => Event::Have {
                    peer: peer.clone(),
                    frontier,
                },
                Ok(Frame::Changes { lines }) => Event::Changes {
                    peer: peer.clone(),
                    lines,
                },
                Ok(Frame::Hello { .. }) => continue,
                Err(_) => break,
            };
            if events.try_send(event).is_err() {
                break;
            }
        }
        let _ = ended.send((peer, serial));
    });
    Open {
        to_send,
        tasks: [writer, reader],
        serial,
    }
}
