use tokio::sync::watch;

use zebra_chain::block;

/// A helper type to determine the best non-finalized chain tip block height.
///
/// The block height is determined based on the current finalized block height and the current best
/// non-finalized chain's tip block height. The height is made available from a [`watch::Receiver`].
#[derive(Debug)]
pub struct BestTipHeight {
    finalized: block::Height,
    non_finalized: Option<block::Height>,
    sender: watch::Sender<block::Height>,
    // TODO: Replace this with a `watch::Sender::borrow` call once Tokio is updated to 1.0.0
    active_value: block::Height,
}

impl BestTipHeight {
    /// Create a new instance of [`BestTipHeight`] and the [`watch::Receiver`] endpoint for the
    /// current best tip block height.
    pub fn new() -> (Self, watch::Receiver<block::Height>) {
        let genesis_height = block::Height(0);
        let (sender, receiver) = watch::channel(genesis_height);

        (
            BestTipHeight {
                finalized: genesis_height,
                non_finalized: None,
                sender,
                active_value: genesis_height,
            },
            receiver,
        )
    }

    /// Update the current finalized block height.
    ///
    /// May trigger an update to best tip height.
    pub fn set_finalized_height(&mut self, new_height: block::Height) {
        if self.finalized != new_height {
            self.finalized = new_height;
            self.update();
        }
    }

    /// Update the current non-finalized block height.
    ///
    /// May trigger an update to the best tip height.
    pub fn set_best_non_finalized_height(&mut self, new_height: Option<block::Height>) {
        if self.non_finalized != new_height {
            self.non_finalized = new_height;
            self.update();
        }
    }

    /// Possibly send an update to listeners.
    ///
    /// An update is only sent if the current best tip height is different from the last best tip
    /// height that was sent.
    fn update(&mut self) {
        let new_value = match self.non_finalized {
            Some(non_finalized) => self.finalized.max(non_finalized),
            None => self.finalized,
        };

        if new_value != self.active_value {
            let _ = self.sender.send(new_value);
            self.active_value = new_value;
        }
    }
}
