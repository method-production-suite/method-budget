//! Positions in an ordered list (rows in an account, accounts in a
//! category, the topsheet...), following the Fugue tree (Weidner and
//! Kleppmann, 2023) as docs/research/collaboration.md, 3.6 describes.
//!
//! Every position ever made in a list is a node of a tree: a left or right
//! child of some earlier position (or of the virtual root). The list order
//! is the in-order walk of the tree, and it is a pure function of the set
//! of nodes, so every computer that has the same nodes gets the same order.
//! Two people inserting runs of lines at the same spot get one run then the
//! other, never the two zipped together.
//!
//! A node is never removed: an entity that moves away leaves its old
//! position behind as an empty anchor that later inserts may still name.
//! The entity's place is its own move register ([`super::state::Place`]):
//! the tree only orders positions.

use std::collections::{BTreeMap, BTreeSet};

use super::clock::Stamp;

/// A position: the stamp of the op that made it, unique across the log.
pub type PosId = Stamp;

/// Which side of its parent a position sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }

    pub fn parse(s: &str) -> Option<Side> {
        match s {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            _ => None,
        }
    }
}

/// Where a new node hangs: under a position (`None` for the virtual root),
/// on a side. Computed by the inserter from its own tree
/// ([`SeqTree::after`]) and carried in the op, so every reader hangs it in
/// the same place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Node {
    pub parent: Option<PosId>,
    pub side: Side,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Kids {
    left: BTreeSet<PosId>,
    right: BTreeSet<PosId>,
}

/// The positions of one list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeqTree {
    nodes: BTreeMap<PosId, Node>,
    kids: BTreeMap<Option<PosId>, Kids>,
}

impl SeqTree {
    pub fn new() -> SeqTree {
        SeqTree::default()
    }

    pub fn contains(&self, id: &PosId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Where the node of `id` hangs, if it exists.
    pub fn node(&self, id: &PosId) -> Option<Node> {
        self.nodes.get(id).copied()
    }

    /// Hangs `id` at `node`. `false` (and nothing changes) when the parent
    /// isn't in the tree. Adding a node that is already there with the same
    /// parent and side is fine; with another parent it is refused.
    pub fn add(&mut self, id: PosId, node: Node) -> bool {
        if let Some(have) = self.nodes.get(&id) {
            return *have == node;
        }
        if let Some(p) = &node.parent {
            if !self.nodes.contains_key(p) {
                return false;
            }
        }
        self.nodes.insert(id, node);
        let kids = self.kids.entry(node.parent).or_default();
        match node.side {
            Side::Left => kids.left.insert(id),
            Side::Right => kids.right.insert(id),
        };
        true
    }

    fn kids(&self, of: Option<PosId>) -> Option<&Kids> {
        self.kids.get(&of)
    }

    /// Where a new node goes to sit right after `left` (`None`: at the very
    /// start), by Fugue's rule: a right child of `left` when `left` has no
    /// right children yet, otherwise a left child of the position that
    /// follows `left`. `left` must be in the tree (or `None`).
    pub fn after(&self, left: Option<PosId>) -> Node {
        let kids = self.kids(left);
        let Some(first_right) = kids.and_then(|k| k.right.iter().next().copied()) else {
            return Node {
                parent: left,
                side: Side::Right,
            };
        };
        // The successor of `left` is the leftmost node of its first right
        // child's subtree.
        let mut r = first_right;
        while let Some(l) = self
            .kids(Some(r))
            .and_then(|k| k.left.iter().next().copied())
        {
            r = l;
        }
        Node {
            parent: Some(r),
            side: Side::Left,
        }
    }

    /// Every position in list order (empty anchors included).
    pub fn order(&self) -> Vec<PosId> {
        enum Frame {
            Visit(Option<PosId>),
            Emit(PosId),
        }
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = vec![Frame::Visit(None)];
        while let Some(f) = stack.pop() {
            match f {
                Frame::Visit(n) => {
                    let kids = self.kids(n);
                    if let Some(k) = kids {
                        for r in k.right.iter().rev() {
                            stack.push(Frame::Visit(Some(*r)));
                        }
                    }
                    if let Some(id) = n {
                        stack.push(Frame::Emit(id));
                    }
                    if let Some(k) = kids {
                        for l in k.left.iter().rev() {
                            stack.push(Frame::Visit(Some(*l)));
                        }
                    }
                }
                Frame::Emit(id) => out.push(id),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::ids::ActorId;

    fn actor(n: u8) -> ActorId {
        let mut a = [0u8; 32];
        a[0] = n;
        ActorId(a)
    }

    fn stamp(lamport: u64, a: u8) -> Stamp {
        Stamp {
            lamport,
            actor: actor(a),
            seq: lamport,
            index: 0,
        }
    }

    /// Appends `n` nodes one after another, as one person typing lines.
    fn run(tree: &mut SeqTree, start: u64, a: u8, n: u64, after: Option<PosId>) -> Vec<PosId> {
        let mut left = after;
        let mut out = Vec::new();
        for i in 0..n {
            let id = stamp(start + i, a);
            let node = tree.after(left);
            assert!(tree.add(id, node));
            out.push(id);
            left = Some(id);
        }
        out
    }

    #[test]
    fn sequential_inserts_keep_their_order() {
        let mut t = SeqTree::new();
        let ids = run(&mut t, 1, 1, 5, None);
        assert_eq!(t.order(), ids);
        // Inserting at the start twice: the latest is first.
        let x = stamp(10, 1);
        t.add(x, t.after(None));
        let y = stamp(11, 1);
        t.add(y, t.after(None));
        assert_eq!(t.order()[..2], [y, x]);
        // Inserting between two existing nodes lands between them.
        let z = stamp(12, 1);
        t.add(z, t.after(Some(ids[1])));
        let order = t.order();
        let at = |id| order.iter().position(|p| *p == id).unwrap();
        assert_eq!(at(z), at(ids[1]) + 1);
        assert_eq!(at(ids[2]), at(z) + 1);
    }

    #[test]
    fn concurrent_runs_do_not_interleave_and_converge() {
        let mut base = SeqTree::new();
        let ids = run(&mut base, 1, 1, 3, None);
        // Alex and Jordan each insert a run after line 1, apart.
        let mut alex = base.clone();
        let a_run = run(&mut alex, 100, 2, 4, Some(ids[0]));
        let mut jordan = base.clone();
        let j_run = run(&mut jordan, 200, 3, 3, Some(ids[0]));
        // Each applies the other's nodes (in causal order: a node needs
        // its parent, which the log guarantees by delivering each actor's
        // changes in sequence).
        for id in &j_run {
            assert!(alex.add(*id, jordan.node(id).unwrap()));
        }
        assert!(!jordan.add(a_run[3], alex.node(&a_run[3]).unwrap()));
        for id in &a_run {
            assert!(jordan.add(*id, alex.node(id).unwrap()));
        }
        assert_eq!(alex.order(), jordan.order());
        let order = alex.order();
        let pos = |id: &PosId| order.iter().position(|p| p == id).unwrap();
        // Each run is contiguous.
        for w in a_run.windows(2) {
            assert_eq!(pos(&w[1]), pos(&w[0]) + 1);
        }
        for w in j_run.windows(2) {
            assert_eq!(pos(&w[1]), pos(&w[0]) + 1);
        }
        assert_eq!(order[0], ids[0]);
        assert_eq!(*order.last().unwrap(), ids[2]);
    }

    #[test]
    fn missing_parent_is_refused() {
        let mut t = SeqTree::new();
        let ghost = stamp(99, 9);
        assert!(!t.add(
            stamp(1, 1),
            Node {
                parent: Some(ghost),
                side: Side::Right
            }
        ));
        assert!(t.is_empty());
    }
}
