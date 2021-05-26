use proptest::{collection::vec, prelude::*};

use zebra_chain::serialization::arbitrary::datetime_full;

use super::super::validate_addrs;
use crate::types::MetaAddr;

proptest! {
    /// Test that validated gossiped peers never have a `last_seen` time that's in the future.
    #[test]
    fn no_last_seen_times_are_in_the_future(
        gossiped_peers in vec(MetaAddr::gossiped_strategy(), 1..10),
        last_seen_limit in datetime_full(),
    ) {
        zebra_test::init();

        let validated_peers = validate_addrs(gossiped_peers, last_seen_limit);

        for peer in validated_peers {
            // Check that malicious peers can't control Zebra's future connections
            //
            // Compare timestamps, allowing an extra second, to account for `chrono` leap seconds:
            // See https://docs.rs/chrono/0.4.19/chrono/naive/struct.NaiveTime.html#leap-second-handling
            prop_assert!(peer.get_last_seen().timestamp() <= last_seen_limit.timestamp() + 1,
                         "peer timestamp {} was greater than limit {}",
                         peer.get_last_seen().timestamp(),
                         last_seen_limit.timestamp());

            /* These tests won't pass until PR #2203 is merged

            use zebra_chain::serialization::{ZcashDeserialize, ZcashSerialize};

            // Check that malicious peers can't make Zebra send bad times to other peers
            // (after Zebra's standard sanitization)
            let sanitized_peer = peer.sanitize();

            // Check that sanitization doesn't put times in the future
            prop_assert!(sanitized_peer.get_last_seen().timestamp() <= last_seen_limit.timestamp() + 1,
                         "sanitized peer timestamp {} was greater than limit {}, original timestamp: {}",
                         sanitized_peer.get_last_seen().timestamp(),
                         last_seen_limit.timestamp(),
                         peer.get_last_seen().timestamp());

            // Check that malicious peers can't make Zebra's serialization fail
            let addr_bytes = peer.zcash_serialize_to_vec();
            prop_assert!(addr_bytes.is_ok(),
                         "unexpected serialization error: {:?}, original timestamp: {}, sanitized timestamp: {}",
                         addr_bytes,
                         peer.get_last_seen().timestamp(),
                         sanitized_peer.get_last_seen().timestamp());

            // Assume other implementations deserialize like Zebra
            let deserialized_peer = MetaAddr::zcash_deserialize(addr_bytes.unwrap().as_slice());
            prop_assert!(deserialized_peer.is_ok(),
                         "unexpected deserialization error: {:?}, original timestamp: {}, sanitized timestamp: {}",
                         deserialized_peer,
                         peer.get_last_seen().timestamp(),
                         sanitized_peer.get_last_seen().timestamp());
            let deserialized_peer = deserialized_peer.unwrap();

            // Check that serialization hasn't modified the address
            // (like the sanitized round-trip test)
            prop_assert_eq!(sanitized_peer, deserialized_peer);

            // Check that sanitization, serialization, and deserialization don't
            // put times in the future
            prop_assert!(deserialized_peer.get_last_seen().timestamp() <= last_seen_limit.timestamp() + 1,
                         "deserialized peer timestamp {} was greater than limit {}, original timestamp: {}, sanitized timestamp: {}",
                         deserialized_peer.get_last_seen().timestamp(),
                         last_seen_limit.timestamp(),
                         peer.get_last_seen().timestamp(),
                         sanitized_peer.get_last_seen().timestamp());
             */
        }
    }
}
