use std::net::SocketAddr;

use chrono::{TimeZone, Utc};
use proptest::prelude::*;

use super::super::validate_addrs;
use crate::types::{MetaAddr, PeerServices};

proptest! {
    #[test]
    fn no_last_seen_times_are_in_the_future(gossiped_peers in
        any::<Vec<(SocketAddr, u32)>>().prop_map(|property_seeds| {
            property_seeds
                .into_iter()
                .map(|(address, last_seen)| {
                    MetaAddr::new_gossiped_meta_addr(
                        address,
                        PeerServices::NODE_NETWORK,
                        Utc.timestamp(last_seen.into(), 0),
                    )
                })
        })
    ) {
        zebra_test::init();

        let last_seen_limit = Utc::now();

        let validated_peers = validate_addrs(gossiped_peers, last_seen_limit);

        for peer in validated_peers {
            prop_assert![peer.get_last_seen() <= last_seen_limit];
        }
    }
}
