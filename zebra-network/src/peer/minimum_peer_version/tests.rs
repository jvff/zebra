use std::sync::Arc;

use proptest::prelude::*;
use tokio::sync::watch;

use zebra_chain::{block, chain_tip::ChainTip, parameters::Network, transaction};

use crate::{peer::MinimumPeerVersion, protocol::external::types::Version};

proptest! {
    /// Test if the calculated minimum peer version is correct.
    #[test]
    fn minimum_peer_version_is_correct(
        network in any::<Network>(),
        block_height in any::<Option<block::Height>>(),
    ) {
        let (chain_tip, best_tip_height) = MockChainTip::new();
        let mut minimum_peer_version = MinimumPeerVersion::new(chain_tip, network);

        best_tip_height
            .send(block_height)
            .expect("receiving endpoint lives as long as `minimum_peer_version`");

        let expected_minimum_version = Version::min_remote_for_height(network, block_height);

        prop_assert_eq!(minimum_peer_version.current(), expected_minimum_version);
    }

    /// Test if the calculated minimum peer version changes with the tip height.
    #[test]
    fn minimum_peer_version_is_updated_with_chain_tip(
        network in any::<Network>(),
        block_heights in any::<Vec<Option<block::Height>>>(),
    ) {
        let (chain_tip, best_tip_height) = MockChainTip::new();
        let mut minimum_peer_version = MinimumPeerVersion::new(chain_tip, network);

        for block_height in block_heights {
            best_tip_height
                .send(block_height)
                .expect("receiving endpoint lives as long as `minimum_peer_version`");

            let expected_minimum_version = Version::min_remote_for_height(network, block_height);

            prop_assert_eq!(minimum_peer_version.current(), expected_minimum_version);
        }
    }
}

/// A mock [`ChainTip`] implementation that allows setting the `best_tip_height` externally.
struct MockChainTip {
    best_tip_height: watch::Receiver<Option<block::Height>>,
}

impl MockChainTip {
    /// Create a new [`MockChainTip`].
    ///
    /// Returns the [`MockChainTip`] instance and the endpoint to modiy the current best tip
    /// height.
    ///
    /// Initially, the best tip height is [`None`].
    pub fn new() -> (Self, watch::Sender<Option<block::Height>>) {
        let (sender, receiver) = watch::channel(None);

        let mock_chain_tip = MockChainTip {
            best_tip_height: receiver,
        };

        (mock_chain_tip, sender)
    }
}

impl ChainTip for MockChainTip {
    fn best_tip_height(&self) -> Option<block::Height> {
        *self.best_tip_height.borrow()
    }

    fn best_tip_hash(&self) -> Option<block::Hash> {
        unreachable!("Method not used in `MinimumPeerVersion` tests");
    }

    fn best_tip_mined_transaction_ids(&self) -> Arc<[transaction::Hash]> {
        unreachable!("Method not used in `MinimumPeerVersion` tests");
    }
}
