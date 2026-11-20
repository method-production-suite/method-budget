//! The live link over method-sync (decisions.md, entry 98).

use super::*;

impl SharedDoc {
    // The live link (decisions.md, entry 98)

    /// The budget this log belongs to, which both ends of a link must share.
    pub fn live_budget(&self) -> Result<String, String> {
        self.shared.dir.info().map(|i| i.budget_id).map_err(|e| e.0)
    }

    /// Turns live mode on for this person, on this computer, for this budget,
    /// with a mesh from [`live_mesh`]. The app comes here only after showing
    /// what it sends. Nothing connects before it, and the heartbeat with the
    /// address is written now.
    pub fn live_with(&mut self, mesh: Mesh) {
        if self.live.is_some() || self.replaced.is_some() {
            return;
        }
        self.live = Some(Live {
            mesh,
            offered: 0,
            peers: BTreeMap::new(),
            linked: HashSet::new(),
            cells: HashMap::new(),
            selections: HashMap::new(),
        });
        self.heartbeat(Instant::now(), true);
    }

    /// Turns live mode off: the link closes, and the address comes out of
    /// the heartbeat file.
    pub fn live_stop(&mut self) {
        if self.live.take().is_some() {
            self.heartbeat(Instant::now(), true);
        }
    }

    /// Whether this person has live mode on.
    pub fn live_on(&self) -> bool {
        self.live.is_some()
    }

    /// How many people this computer has a live link to.
    pub fn live_connected(&self) -> usize {
        self.live.as_ref().map_or(0, |l| l.mesh.connected())
    }

    /// The cell a person is on, when their heartbeat names a linked computer
    /// that said so. Someone removed shows none.
    pub(super) fn live_cell(&self, actor: ActorId) -> Option<Cell> {
        let live = self.live.as_ref()?;
        if self.shared.log.removed(actor) {
            return None;
        }
        let id = live_id(self.people.get(&actor)?.live.as_deref()?)?;
        live.cells.get(&id).cloned()
    }

    /// The cells a person has selected, as [`Self::live_cell`] finds theirs.
    pub(super) fn live_selection(&self, actor: ActorId) -> Vec<Cell> {
        let Some(live) = self.live.as_ref() else {
            return Vec::new();
        };
        if self.shared.log.removed(actor) {
            return Vec::new();
        }
        self.people
            .get(&actor)
            .and_then(|p| live_id(p.live.as_deref()?))
            .and_then(|id| live.selections.get(&id).cloned())
            .unwrap_or_default()
    }

    /// A live link to this person's computer is open. Someone removed has none.
    pub(super) fn live_linked(&self, actor: ActorId) -> bool {
        let Some(live) = self.live.as_ref() else {
            return false;
        };
        if self.shared.log.removed(actor) {
            return false;
        }
        self.people
            .get(&actor)
            .and_then(|p| p.live.as_deref())
            .and_then(live_id)
            .is_some_and(|id| live.linked.contains(&id))
    }

    /// Who should be connected: members whose heartbeat gives an address and
    /// is recent, or who have a link open already. Never someone removed.
    fn live_peers(&self) -> BTreeMap<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let me = self.actor();
        self.people
            .iter()
            .filter(|(a, _)| **a != me && !self.shared.log.removed(**a))
            .filter_map(|(_, p)| {
                let address = p.live.clone()?;
                let id = live_id(&address)?;
                let recent =
                    iso_secs(&p.at).is_some_and(|at| now.saturating_sub(at) < LIVE_STALE.as_secs());
                let open = self.live.as_ref().is_some_and(|l| l.linked.contains(&id));
                (recent || open).then_some((id, address))
            })
            .collect()
    }

    /// Takes in what arrived on the links, and answers anyone who said what
    /// they hold. Changes are judged by the log exactly as a segment's are.
    pub(super) fn live_receive(&mut self) -> usize {
        let Some(live) = &mut self.live else { return 0 };
        let mut lines = Vec::new();
        let mut asked = Vec::new();
        for event in live.mesh.events() {
            match event {
                LiveEvent::Up { peer } => {
                    live.linked.insert(peer.clone());
                    let frontier = self
                        .shared
                        .log
                        .frontier()
                        .0
                        .iter()
                        .map(|(a, s)| (a.to_string(), *s))
                        .collect();
                    live.mesh.send_to(&peer, Frame::Have { frontier });
                    live.mesh.send_to(
                        &peer,
                        Frame::Cursor {
                            cell: self.cell.clone(),
                            selection: self.selection.clone(),
                        },
                    );
                }
                LiveEvent::Have { peer, frontier } => asked.push((peer, frontier)),
                LiveEvent::Changes { lines: more, .. } => lines.extend(more),
                LiveEvent::Cursor {
                    peer,
                    cell,
                    selection,
                } => {
                    match checked_selection(selection) {
                        selection if selection.is_empty() => live.selections.remove(&peer),
                        selection => live.selections.insert(peer.clone(), selection),
                    };
                    match cell.and_then(checked_cell) {
                        Some(cell) => live.cells.insert(peer, cell),
                        None => live.cells.remove(&peer),
                    };
                }
                LiveEvent::Down { peer } => {
                    live.linked.remove(&peer);
                    live.cells.remove(&peer);
                    live.selections.remove(&peer);
                }
            }
        }
        for (peer, frontier) in asked {
            self.live_answer(&peer, &frontier);
        }
        let changes = lines
            .iter()
            .filter_map(|l| flog::change_from_json(l).ok())
            .collect();
        flog::apply_changes(&mut self.shared.log, changes).applied
    }

    /// Whether a change may go on the link: this computer's own only once a
    /// segment holds it. If this computer stopped before writing it, it
    /// would use the same number again for a different change, and everyone
    /// who had the first would see a fork.
    fn live_may_send(&self, c: &method_core::log::Change) -> bool {
        c.actor != self.actor() || c.seq <= self.written_seq
    }

    /// Sends a peer the changes it lacks.
    fn live_answer(&self, peer: &str, frontier: &BTreeMap<String, u64>) {
        let Some(live) = &self.live else { return };
        let mut theirs = method_core::log::Frontier::new();
        for (actor, seq) in frontier {
            if let Some(a) = ActorId::parse(actor) {
                theirs.advance(a, *seq);
            }
        }
        let lacking = self.shared.log.since(&theirs);
        let sendable = lacking.into_iter().filter(|c| self.live_may_send(c));
        for lines in live_frames(sendable) {
            live.mesh.send_to(peer, Frame::Changes { lines });
        }
    }

    /// Tells the mesh who should be connected, and offers everyone the
    /// changes it hasn't offered yet.
    pub(super) fn live_send(&mut self) {
        if self.live.is_none() {
            return;
        }
        let peers = self.live_peers();
        let Some(live) = &self.live else { return };
        if live.peers != peers {
            live.mesh.set_peers(peers.clone());
        }
        let offered = live.offered.min(self.shared.log.len());
        let ready: Vec<_> = self
            .shared
            .log
            .changes_from(offered)
            .take_while(|c| self.live_may_send(c))
            .collect();
        let count = ready.len();
        for lines in live_frames(ready.into_iter()) {
            live.mesh.broadcast(Frame::Changes { lines });
        }
        if let Some(live) = &mut self.live {
            live.peers = peers;
            live.offered = offered + count;
        }
    }
}

/// The computer's id in a heartbeat's live address.
fn live_id(address: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(address)
        .ok()?
        .get("id")?
        .as_str()
        .map(str::to_string)
}

/// A cell as the page may read it, or none: what arrives on a link comes from
/// another computer, and only the grid's kinds and short names go further.
pub(super) fn checked_cell(cell: Cell) -> Option<Cell> {
    const LONGEST: usize = 200;
    let short = |s: &str| s.len() <= LONGEST;
    let kinds = ["row", "account", "category", "production_total"];
    (kinds.contains(&cell.kind.as_str())
        && short(&cell.key)
        && cell.parent.as_deref().is_none_or(short)
        && cell.field.as_deref().is_none_or(short))
    .then_some(cell)
}

/// A selection as the page may read it: each cell checked like the cursor's
/// (one that fails is left out), and none at all past [`MOST_SELECTED`].
pub(super) fn checked_selection(selection: Vec<Cell>) -> Vec<Cell> {
    if selection.len() > MOST_SELECTED {
        return Vec::new();
    }
    selection.into_iter().filter_map(checked_cell).collect()
}

/// Changes as frames' worth of the log's own lines. One too large for any
/// frame is left to the folder, which carries it as always.
fn live_frames<'a>(
    changes: impl Iterator<Item = &'a method_core::log::Change>,
) -> Vec<Vec<String>> {
    let mut frames = Vec::new();
    let mut lines = Vec::new();
    let mut bytes = 0;
    for c in changes {
        let line = flog::change_to_json(c);
        if line.len() > LIVE_FRAME_BYTES {
            continue;
        }
        if bytes + line.len() > LIVE_FRAME_BYTES {
            frames.push(std::mem::take(&mut lines));
            bytes = 0;
        }
        bytes += line.len();
        lines.push(line);
    }
    if !lines.is_empty() {
        frames.push(lines);
    }
    frames
}
