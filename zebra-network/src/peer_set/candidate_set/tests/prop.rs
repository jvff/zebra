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
        }
    }
}
