//! Test vectors for MetaAddr.

use std::net::SocketAddr;

use zebra_chain::serialization::{DateTime32, Duration32};

use super::{super::MetaAddr, check};
use crate::{constants::REACHABLE_PEER_DURATION, protocol::types::PeerServices};

/// Margin of error for time-based tests.
///
/// This is a short duration to consider as error due to a test's execution time when comparing
/// [`DateTime32`]s.
const TEST_TIME_ERROR_MARGIN: Duration32 = Duration32::from_seconds(1);

/// Make sure that the sanitize function handles minimum and maximum times.
#[test]
fn sanitize_extremes() {
    zebra_test::init();

    let min_time_entry = MetaAddr {
        addr: "127.0.0.1:8233".parse().unwrap(),
        services: Default::default(),
        untrusted_last_seen: Some(u32::MIN.into()),
        last_response: Some(u32::MIN.into()),
        last_attempt: None,
        last_failure: None,
        last_connection_state: Default::default(),
    };

    let max_time_entry = MetaAddr {
        addr: "127.0.0.1:8233".parse().unwrap(),
        services: Default::default(),
        untrusted_last_seen: Some(u32::MAX.into()),
        last_response: Some(u32::MAX.into()),
        last_attempt: None,
        last_failure: None,
        last_connection_state: Default::default(),
    };

    if let Some(min_sanitized) = min_time_entry.sanitize() {
        check::sanitize_avoids_leaks(&min_time_entry, &min_sanitized);
    }
    if let Some(max_sanitized) = max_time_entry.sanitize() {
        check::sanitize_avoids_leaks(&max_time_entry, &max_sanitized);
    }
}

/// Test if new peer is not recently reachable.
#[test]
fn new_peer_is_not_recently_reachable() {
    zebra_test::init();

    let address = SocketAddr::from(([192, 168, 180, 9], 10_000));
    let peer = MetaAddr::new_alternate(&address, &PeerServices::NODE_NETWORK)
        .into_new_meta_addr()
        .expect("MetaAddrChange can't create a new MetaAddr");

    assert!(!peer.was_recently_reachable());
}

/// Test gossiped peer that is recently reachable.
#[test]
fn gossiped_peer_reportedly_to_be_seen_recently_is_recently_reachable() {
    zebra_test::init();

    let address = SocketAddr::from(([192, 168, 180, 9], 10_000));

    // Report last seen within the reachable interval.
    let offset = REACHABLE_PEER_DURATION
        .checked_sub(TEST_TIME_ERROR_MARGIN)
        .expect("Test margin is too large");
    let last_seen = DateTime32::now()
        .checked_sub(offset)
        .expect("Offset is too large");

    let peer = MetaAddr::new_gossiped_meta_addr(address, PeerServices::NODE_NETWORK, last_seen);

    assert!(peer.was_recently_reachable());
}
