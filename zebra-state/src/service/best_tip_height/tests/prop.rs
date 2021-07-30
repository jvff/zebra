use proptest::prelude::*;

use zebra_chain::block;

use super::super::BestTipHeight;
use crate::constants;

/// Maximum finalized height to use in tests with non-finalized blocks.
const MAX_FINALIZED_HEIGHT_BEFORE_FINAL_REORG: u32 =
    block::Height::MAX.0 - constants::MAX_BLOCK_REORG_HEIGHT;

proptest! {
    #[test]
    fn non_finalized_tip_changes_best_tip(non_finalized_height in any::<block::Height>()) {
        let (mut best_tip_height, receiver) = BestTipHeight::new();

        best_tip_height.set_best_non_finalized_height(Some(non_finalized_height));

        assert_eq!(*receiver.borrow(), Some(non_finalized_height));
    }

    #[test]
    fn finalized_tip_changes_best_tip(finalized_height in any::<block::Height>()) {
        let (mut best_tip_height, receiver) = BestTipHeight::new();

        best_tip_height.set_finalized_height(finalized_height);

        assert_eq!(*receiver.borrow(), Some(finalized_height));
    }

    #[test]
    fn non_finalized_tip_replaces_finalized_tip(
        finalized_height in (0..=MAX_FINALIZED_HEIGHT_BEFORE_FINAL_REORG).prop_map(block::Height),
        non_finalized_blocks in 1..=constants::MAX_BLOCK_REORG_HEIGHT,
    ) {
        let (mut best_tip_height, receiver) = BestTipHeight::new();

        let non_finalized_height = block::Height(finalized_height.0 + non_finalized_blocks);

        best_tip_height.set_finalized_height(finalized_height);
        best_tip_height.set_best_non_finalized_height(Some(non_finalized_height));

        assert_eq!(*receiver.borrow(), Some(non_finalized_height));
    }

    #[test]
    fn finalized_tip_replaces_best_tip(
        non_finalized_height in
            (0..=MAX_FINALIZED_HEIGHT_BEFORE_FINAL_REORG).prop_map(block::Height),
        finalized_blocks_that_skip_non_finalized_state in 1..=constants::MAX_BLOCK_REORG_HEIGHT,
    ) {
        let (mut best_tip_height, receiver) = BestTipHeight::new();

        let finalized_height = block::Height(
            non_finalized_height.0 + finalized_blocks_that_skip_non_finalized_state,
        );

        best_tip_height.set_best_non_finalized_height(Some(non_finalized_height));
        best_tip_height.set_finalized_height(finalized_height);

        assert_eq!(*receiver.borrow(), Some(finalized_height));
    }

    #[test]
    fn best_tip_value_is_heighest_of_finalized_and_non_finalized_heights(
        finalized_height in any::<Option<block::Height>>(),
        non_finalized_height in any::<Option<block::Height>>(),
    ) {
        let (mut best_tip_height, receiver) = BestTipHeight::new();

        best_tip_height.set_best_non_finalized_height(non_finalized_height);

        if let Some(finalized_height) = finalized_height {
            best_tip_height.set_finalized_height(finalized_height);
        }

        let expected_height = match (finalized_height, non_finalized_height) {
            (Some(finalized_height), Some(non_finalized_height)) => {
                Some(finalized_height.max(non_finalized_height))
            }
            (finalized_height, None) => finalized_height,
            (None, non_finalized_height) => non_finalized_height,
        };

        prop_assert_eq!(*receiver.borrow(), expected_height);
    }
}
