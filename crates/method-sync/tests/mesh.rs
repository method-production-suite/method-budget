//! Meshes on this computer's loopback address, with no relay: nothing here
//! leaves the computer.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use method_sync::mesh::{Event, Mesh, RETRY};
use method_sync::Frame;

const WAIT: Duration = Duration::from_secs(20);

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < WAIT, "not seen in time: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn mesh(seed: u8) -> Mesh {
    Mesh::start_loopback([seed; 32], "budget-1").unwrap()
}

fn know(a: &Mesh, others: &[&Mesh]) {
    let peers: BTreeMap<String, String> = others
        .iter()
        .map(|m| (m.id(), m.address().unwrap()))
        .collect();
    a.set_peers(peers);
}

fn linked() -> (Mesh, Mesh) {
    let (a, b) = (mesh(1), mesh(2));
    know(&a, &[&b]);
    know(&b, &[&a]);
    until("the link", || a.connected() == 1 && b.connected() == 1);
    (a, b)
}

/// Waits for a frame's event, keeping whatever else arrived out of the way.
fn wait_for(m: &Mesh, what: &str, want: impl Fn(&Event) -> bool) -> Event {
    let start = Instant::now();
    loop {
        if let Some(e) = m.events().into_iter().find(&want) {
            return e;
        }
        assert!(start.elapsed() < WAIT, "not seen in time: {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn frames_cross_both_ways() {
    let (a, b) = linked();
    a.broadcast(Frame::Changes {
        lines: vec!["one".into()],
    });
    let got = wait_for(&b, "a's change", |e| matches!(e, Event::Changes { .. }));
    assert_eq!(
        got,
        Event::Changes {
            peer: a.id(),
            lines: vec!["one".into()]
        }
    );

    let frontier = BTreeMap::from([("x".to_string(), 3)]);
    b.send_to(
        &a.id(),
        Frame::Have {
            frontier: frontier.clone(),
        },
    );
    let got = wait_for(&a, "b's frontier", |e| matches!(e, Event::Have { .. }));
    assert_eq!(
        got,
        Event::Have {
            peer: b.id(),
            frontier
        }
    );
}

/// A person removed from the budget stops receiving changes at once.
#[test]
fn a_peer_taken_off_the_list_loses_its_link() {
    let (a, b) = linked();
    a.set_peers(BTreeMap::new());
    until("the link gone on a", || a.connected() == 0);
    until("the link gone on b", || b.connected() == 0);
    a.broadcast(Frame::Changes {
        lines: vec!["secret".into()],
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(!b
        .events()
        .iter()
        .any(|e| matches!(e, Event::Changes { .. })));
}

/// Dropping the mesh is the only way out, and it leaves nothing online.
#[test]
fn dropping_a_mesh_ends_its_links_and_takes_it_offline() {
    let (a, b) = linked();
    let address = a.address().unwrap();
    let id = a.id();
    drop(a);
    until("b hears", || b.connected() == 0);
    // Dialing the old address gets nowhere.
    let c = mesh(1 + 2);
    c.set_peers(BTreeMap::from([(id, address)]));
    std::thread::sleep(RETRY + Duration::from_millis(500));
    assert_eq!(c.connected(), 0);
}

/// A refused connection is tried again: here `b` doesn't know `a` at first.
#[test]
fn a_refused_connection_is_tried_again() {
    let (a, b) = (mesh(1), mesh(2));
    let (first, second) = if a.id() < b.id() { (&a, &b) } else { (&b, &a) };
    know(first, &[second]);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(first.connected(), 0, "connected before being allowed");
    know(second, &[first]);
    until("the link after a retry", || {
        a.connected() == 1 && b.connected() == 1
    });
}

/// One mesh per budget: the same two computers with different budgets never link.
#[test]
fn a_different_budget_never_links() {
    let a = Mesh::start_loopback([1; 32], "budget-1").unwrap();
    let b = Mesh::start_loopback([2; 32], "budget-2").unwrap();
    know(&a, &[&b]);
    know(&b, &[&a]);
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!((a.connected(), b.connected()), (0, 0));
}

/// A link that ends comes back when the other side returns.
#[test]
fn a_link_comes_back_after_the_other_side_returns() {
    let (a, b) = linked();
    b.set_peers(BTreeMap::new());
    until("the link gone", || a.connected() == 0 && b.connected() == 0);
    know(&b, &[&a]);
    until("the link back", || a.connected() == 1 && b.connected() == 1);
}
