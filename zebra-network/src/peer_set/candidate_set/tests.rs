//! [`CandidateSet`] tests.

use std::{
    future,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

use chrono::{Duration, Utc};
use tower::Service;
use tracing::Level;

use super::{AddressBook, BoxError, CandidateSet, MetaAddr, Request, Response};
use crate::{protocol::types::PeerServices, Config};

/// Test if gossiped peers' reported last seen time is properly normalized.
///
/// This is a security issue if this test fails, because a malicious peer can force the local node
/// to use the peers it wants to, by reporting a last seen time that's in the future.
///
/// Note that this issue can also occur due to skewed clocks of different machines.
///
/// For more information, see the respective [issue] on GitHub.
///
/// [issue]: https://github.com/ZcashFoundation/zebra/issues/1871
#[tokio::test]
async fn gossiped_peers_from_a_single_peer_doesnt_get_full_precedence() {
    // Use port numbers to mark peers to distinguish malicious ones from normal ones in the test
    const MALICIOUS_PEER_PORT: u16 = 20_666;
    const NORMAL_PEER_PORT: u16 = 20_111;
    const SEED_PEER_PORT: u16 = 20_000;

    let now = Utc::now();

    let malicious_peer = MetaAddr::new_responded(
        &SocketAddr::from((IpAddr::from([192, 168, 1, 1]), SEED_PEER_PORT)),
        &PeerServices::NODE_NETWORK,
    );

    let normal_peer = MetaAddr::new_responded(
        &SocketAddr::from((IpAddr::from([192, 168, 1, 2]), SEED_PEER_PORT)),
        &PeerServices::NODE_NETWORK,
    );

    let mut address_book = AddressBook::new(&Config::default(), span!(Level::WARN, "Test span"));

    address_book.extend(vec![normal_peer, malicious_peer]);

    let peers_gossiped_by_normal_peer = vec![
        MetaAddr::new_gossiped_meta_addr(
            SocketAddr::from((IpAddr::from([192, 168, 1, 101]), NORMAL_PEER_PORT)),
            PeerServices::NODE_NETWORK,
            now - Duration::minutes(15),
        ),
        MetaAddr::new_gossiped_meta_addr(
            SocketAddr::from((IpAddr::from([192, 168, 1, 102]), NORMAL_PEER_PORT)),
            PeerServices::NODE_NETWORK,
            now - Duration::minutes(45),
        ),
    ];

    let peers_gossiped_by_malicious_peer = vec![
        MetaAddr::new_gossiped_meta_addr(
            SocketAddr::from((IpAddr::from([192, 168, 2, 101]), MALICIOUS_PEER_PORT)),
            PeerServices::NODE_NETWORK,
            now + Duration::minutes(30),
        ),
        MetaAddr::new_gossiped_meta_addr(
            SocketAddr::from((IpAddr::from([192, 168, 2, 102]), MALICIOUS_PEER_PORT)),
            PeerServices::NODE_NETWORK,
            now + Duration::minutes(31),
        ),
    ];

    let peer_service = MockPeerService(vec![
        Response::Peers(peers_gossiped_by_normal_peer),
        Response::Peers(peers_gossiped_by_malicious_peer),
    ]);

    let mut candidate_set = CandidateSet::new(Arc::new(Mutex::new(address_book)), peer_service);

    candidate_set
        .update_initial(2)
        .await
        .expect("gossiping to succeed");

    let first_reconnect_target = candidate_set
        .next()
        .await
        .expect("a reconnect target address");
    let second_reconnect_target = candidate_set
        .next()
        .await
        .expect("a second reconnect target address");

    assert_ne!(first_reconnect_target.addr.port(), SEED_PEER_PORT);
    assert_ne!(second_reconnect_target.addr.port(), SEED_PEER_PORT);
    assert_ne!(
        first_reconnect_target.addr.port(),
        second_reconnect_target.addr.port()
    );
}

pub struct MockPeerService(Vec<Response>);

impl Service<Request> for MockPeerService {
    type Response = Response;
    type Error = BoxError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: Request) -> Self::Future {
        let next_response = self
            .0
            .pop()
            .expect("Mock peer service was called too many times");

        future::ready(Ok(next_response))
    }
}
