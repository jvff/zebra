use proptest::{collection::vec, prelude::*};

use zebra_chain::serialization::arbitrary::datetime_full;

use super::super::validate_addrs;
use crate::types::MetaAddr;
use chrono::{DateTime, Utc};

/// Convert `date` to a string, including both calendar and numeric timestamp.
fn display_with_timestamp(date: DateTime<Utc>) -> String {
    format!("{} ({})", date.to_string(), date.timestamp())
}

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
            prop_assert!(peer.get_last_seen() <= last_seen_limit,
                         "peer last seen {} was greater than limit {}",
                         display_with_timestamp(peer.get_last_seen()),
                         display_with_timestamp(last_seen_limit));

            /* These tests won't pass until PR #2203 is merged

            use zebra_chain::serialization::{ZcashDeserialize, ZcashSerialize};

            // Check that malicious peers can't make Zebra send bad times to other peers
            // (after Zebra's standard sanitization)
            let sanitized_peer = peer.sanitize();

            // Check that sanitization doesn't put times in the future
            prop_assert!(sanitized_peer.get_last_seen() <= last_seen_limit,
                         "sanitized peer timestamp {} was greater than limit {}, original timestamp: {}",
                         display_with_timestamp(sanitized_peer.get_last_seen()),
                         display_with_timestamp(last_seen_limit),
                         display_with_timestamp(peer.get_last_seen()));

            // Check that malicious peers can't make Zebra's serialization fail
            let addr_bytes = peer.zcash_serialize_to_vec();
            prop_assert!(addr_bytes.is_ok(),
                         "unexpected serialization error: {:?}, original timestamp: {}, sanitized timestamp: {}",
                         addr_bytes,
                         display_with_timestamp(peer.get_last_seen()),
                         display_with_timestamp(sanitized_peer.get_last_seen()));

            // Assume other implementations deserialize like Zebra
            let deserialized_peer = MetaAddr::zcash_deserialize(addr_bytes.unwrap().as_slice());
            prop_assert!(deserialized_peer.is_ok(),
                         "unexpected deserialization error: {:?}, original timestamp: {}, sanitized timestamp: {}",
                         deserialized_peer,
                         display_with_timestamp(peer.get_last_seen()),
                         display_with_timestamp(sanitized_peer.get_last_seen()));
            let deserialized_peer = deserialized_peer.unwrap();

            // Check that serialization hasn't modified the address
            // (like the sanitized round-trip test)
            prop_assert_eq!(sanitized_peer, deserialized_peer);

            // Check that sanitization, serialization, and deserialization don't
            // put times in the future
            prop_assert!(deserialized_peer.get_last_seen() <= last_seen_limit,
                         "deserialized peer timestamp {} was greater than limit {}, original timestamp: {}, sanitized timestamp: {}",
                         display_with_timestamp(deserialized_peer.get_last_seen()),
                         display_with_timestamp(last_seen_limit),
                         display_with_timestamp(peer.get_last_seen()),
                         display_with_timestamp(sanitized_peer.get_last_seen()));
             */
        }
    }
}
