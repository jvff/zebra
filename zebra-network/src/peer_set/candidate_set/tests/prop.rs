use chrono::Utc;
use proptest::{collection::vec, prelude::*};

use super::super::validate_addrs;
use crate::types::MetaAddr;

proptest! {
    /// Test that validated gossiped peers never have a `last_seen` time that's in the future.
    #[test]
    fn no_last_seen_times_are_in_the_future(
        gossiped_peers in vec(MetaAddr::gossiped_strategy(), 1..10),
    ) {
        zebra_test::init();

        let last_seen_limit = Utc::now();

        let validated_peers = validate_addrs(gossiped_peers, last_seen_limit);

        for peer in validated_peers {
            prop_assert![peer.get_last_seen() <= last_seen_limit];
        }
    }
}
