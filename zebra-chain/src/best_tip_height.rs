//! Real-time access to the current best non-finalized tip height and the finalized tip height.

use crate::block;

/// Access to the current best non-finalized chain tip height and the finalized chain tip height.
pub trait BestTipHeight {
    /// Retrieve the current best chain tip height.
    fn best_tip_height(&self) -> block::Height;
}

/// Allow using a dummy best tip height when testing.
///
/// This dummy implementation will always return the height of the genesis block (0).
#[cfg(any(test, feature = "proptest-impl"))]
impl BestTipHeight for () {
    fn best_tip_height(&self) -> block::Height {
        block::Height(0)
    }
}
