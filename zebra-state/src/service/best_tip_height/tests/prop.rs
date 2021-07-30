use proptest::prelude::*;

use zebra_chain::block;

use super::super::BestTipHeight;

proptest! {
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
