use tokio::sync::watch;

use zebra_chain::{best_tip_height::BestTipHeight, block};

/// Receiver end to watch the current non-finalized best tip height and the finalized tip height.
#[derive(Clone, Debug)]
pub struct BestTipHeightReceiver {
    finalized: watch::Receiver<block::Height>,
    non_finalized: watch::Receiver<Option<block::Height>>,
}

impl BestTipHeightReceiver {
    /// Create the endpoints for the best tip height.
    ///
    /// Creates a [`BestTipHeight`] to act as the receiver endpoint, a
    /// [`watch::Sender<block::Height>`][watch::Sender] to act as the finalized tip sender endpoint,
    /// and a [`watch::Sender<Option<block::Height>>`][watch::Sender] to act as the best
    /// non-finalized tip sender endpoint.
    pub fn new() -> (
        Self,
        watch::Sender<block::Height>,
        watch::Sender<Option<block::Height>>,
    ) {
        let (finalized_sender, finalized_receiver) = watch::channel(block::Height(1));
        let (non_finalized_sender, non_finalized_receiver) = watch::channel(None);

        let receiver = BestTipHeightReceiver {
            finalized: finalized_receiver,
            non_finalized: non_finalized_receiver,
        };

        (receiver, finalized_sender, non_finalized_sender)
    }
}

impl BestTipHeight for BestTipHeightReceiver {
    /// Retrieve the current best chain tip height.
    ///
    /// Prioritizes the best non-finalized chain tip. If there are no known non-finalized blocks,
    /// this falls back to the finalized tip height.
    fn best_tip_height(&self) -> block::Height {
        // Bind the borrow guard so that the non-finalized watch channel doesn't update while
        // reading from the finalized watch channel.
        let non_finalized = self.non_finalized.borrow();

        non_finalized.unwrap_or(*self.finalized.borrow())
    }
}
