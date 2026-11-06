//! Two endpoints on this computer's loopback address, with no relay: nothing
//! here leaves the computer.

use std::collections::BTreeMap;
use std::time::Duration;

use method_sync::{Frame, Link, Node, MAX_FRAME};

const WAIT: Duration = Duration::from_secs(15);

async fn pair() -> (Node, Node) {
    let a = Node::start_loopback([1; 32]).await.unwrap();
    let b = Node::start_loopback([2; 32]).await.unwrap();
    a.allow(&[b.id()]).unwrap();
    b.allow(&[a.id()]).unwrap();
    (a, b)
}

/// Connects `a` to `b`. QUIC tells the accepting side about a stream only
/// once the opener writes on it, so each side says hello at the same time.
async fn linked(a: &Node, b: &Node, budget: &str) -> (Link, Link) {
    let address = b.address().unwrap();
    let (mine, theirs) = tokio::join!(
        async {
            let mut link = a.connect(&address).await.unwrap();
            link.hello(budget).await.unwrap();
            link
        },
        async {
            let mut link = b.accept().await.unwrap();
            link.hello(budget).await.unwrap();
            link
        }
    );
    (mine, theirs)
}

#[tokio::test]
async fn frames_cross_both_ways_and_name_the_peer() {
    let (a, b) = pair().await;
    let (mut mine, mut theirs) = tokio::time::timeout(WAIT, linked(&a, &b, "budget-1"))
        .await
        .unwrap();
    assert_eq!(mine.peer(), b.id());
    assert_eq!(theirs.peer(), a.id());

    let have = Frame::Have {
        frontier: BTreeMap::from([("actor".to_string(), 7)]),
    };
    mine.send(&have).await.unwrap();
    assert_eq!(theirs.recv().await.unwrap(), have);

    let changes = Frame::Changes {
        lines: vec!["{\"signed\":\"line\"}".to_string()],
    };
    theirs.send(&changes).await.unwrap();
    assert_eq!(mine.recv().await.unwrap(), changes);
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn a_stranger_is_closed_before_any_frame() {
    let (a, b) = pair().await;
    let stranger = Node::start_loopback([3; 32]).await.unwrap();
    stranger.allow(&[b.id()]).unwrap();
    let address = b.address().unwrap();

    let accepted = tokio::spawn({
        let b = b.clone();
        async move { b.accept().await.map(|l| l.peer()) }
    });
    // The stranger connects and says hello; nobody ever answers.
    if let Ok(mut link) = stranger.connect(&address).await {
        let said = link.hello("budget-1").await;
        assert!(said.is_err(), "a stranger got an answer: {said:?}");
    }
    // A member still gets through afterwards, and only the member.
    let mut link = a.connect(&address).await.unwrap();
    link.send(&Frame::Hello {
        budget: "budget-1".into(),
    })
    .await
    .unwrap();
    let peer = tokio::time::timeout(WAIT, accepted).await.unwrap().unwrap();
    assert_eq!(peer, Some(a.id()));
    for n in [&a, &b, &stranger] {
        n.close().await;
    }
}

/// A member who connects and then says nothing holds up nobody else. The
/// wait is far shorter than the time a silent connection is given.
#[tokio::test]
async fn a_silent_connection_does_not_keep_a_member_waiting() {
    let (a, b) = pair().await;
    let silent = Node::start_loopback([3; 32]).await.unwrap();
    silent.allow(&[b.id()]).unwrap();
    b.allow(&[a.id(), silent.id()]).unwrap();
    let address = b.address().unwrap();

    // `connect` opens a stream but writes nothing, so `b` never sees one.
    let _quiet = silent.connect(&address).await.unwrap();
    let (mine, theirs) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            async {
                let mut l = a.connect(&address).await.unwrap();
                l.hello("budget-1").await.unwrap();
                l
            },
            async {
                let mut l = b.accept().await.unwrap();
                l.hello("budget-1").await.unwrap();
                l
            }
        )
    })
    .await
    .expect("a silent connection kept a member waiting");
    assert_eq!(theirs.peer(), a.id());
    drop((mine, theirs));
    for n in [&a, &b, &silent] {
        n.close().await;
    }
}

#[tokio::test]
async fn closing_a_node_ends_its_accept() {
    let node = Node::start_loopback([5; 32]).await.unwrap();
    node.close().await;
    let ended = tokio::time::timeout(WAIT, node.accept()).await.unwrap();
    assert!(ended.is_none());
}

#[tokio::test]
async fn connecting_to_a_key_not_on_the_list_is_refused_here() {
    let (a, b) = pair().await;
    a.allow(&[]).unwrap();
    let err = a.connect(&b.address().unwrap()).await.unwrap_err();
    assert!(err.0.contains("isn't a member"), "{err}");
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn a_different_budget_is_refused_by_hello() {
    let (a, b) = pair().await;
    let address = b.address().unwrap();
    // Each side keeps its link until both have heard: dropping one ends it.
    let ((mine, _keep_a), (theirs, _keep_b)) = tokio::time::timeout(WAIT, async {
        tokio::join!(
            async {
                let mut l = a.connect(&address).await.unwrap();
                (l.hello("budget-1").await, l)
            },
            async {
                let mut l = b.accept().await.unwrap();
                (l.hello("budget-2").await, l)
            }
        )
    })
    .await
    .unwrap();
    assert!(mine.unwrap_err().0.contains("different budget"));
    assert!(theirs.unwrap_err().0.contains("different budget"));
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn an_oversized_frame_is_refused_before_it_is_read() {
    let (a, b) = pair().await;
    let (mut mine, mut theirs) = tokio::time::timeout(WAIT, linked(&a, &b, "budget-1"))
        .await
        .unwrap();
    mine.send_raw(&((MAX_FRAME as u32) + 1).to_be_bytes())
        .await
        .unwrap();
    let err = tokio::time::timeout(WAIT, theirs.recv())
        .await
        .unwrap()
        .unwrap_err();
    assert!(err.0.contains("too large"), "{err}");

    let big = Frame::Changes {
        lines: vec!["x".repeat(MAX_FRAME)],
    };
    assert!(mine.send(&big).await.is_err());
    a.close().await;
    b.close().await;
}

#[tokio::test]
async fn an_endpoint_publishes_its_address_nowhere() {
    let node = method_sync::Node::start([4; 32], method_sync::Relay::None)
        .await
        .unwrap();
    assert!(!node.publishes_address());
    node.close().await;
}

/// iroh's `portmapper` feature asks the router to open a port (decisions.md,
/// entry 98). This fails if a dependency turns it back on.
#[test]
fn the_crate_that_talks_to_routers_is_not_in_the_build() {
    let lock = include_str!("../../../Cargo.lock");
    for name in ["portmapper", "igd-next"] {
        assert!(
            !lock.contains(&format!("name = \"{name}\"")),
            "{name} is in Cargo.lock"
        );
    }
}

#[test]
fn a_relay_address_is_checked() {
    assert!(method_sync::Relay::own("https://relay.example.com").is_ok());
    assert!(method_sync::Relay::own("not an address").is_err());
}
