use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
};

use zebra_chain::{
    block, orchard, sapling, sprout,
    transaction::{UnminedTx, UnminedTxId},
    transparent,
};
use zebra_consensus::error::TransactionError;

use super::MempoolError;

#[cfg(test)]
pub mod tests;

const MEMPOOL_SIZE: usize = 2;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum State {
    /// Rejected because verification failed.
    Invalid(TransactionError),
    /// An otherwise valid mempool transaction was mined into a block, therefore
    /// no longer belongs in the mempool.
    Confirmed(block::Hash),
    /// Rejected because it has a spend conflict with another transaction already in the mempool.
    SpendConflict,
    /// Stayed in mempool for too long without being mined.
    // TODO(2021-09-09): Implement ZIP-203: Validate Transaction Expiry Height.
    // TODO(2021-09-09): https://github.com/ZcashFoundation/zebra/issues/2387
    Expired,
    /// Transaction fee is too low for the current mempool state.
    LowFee,
    /// Otherwise valid transaction removed from mempool, say because of FIFO
    /// (first in, first out) policy.
    Excess,
}

#[derive(Default)]
pub struct Storage {
    /// The set of verified transactions in the mempool. This is a
    /// cache of size [`MEMPOOL_SIZE`].
    verified: VecDeque<UnminedTx>,

    /// The set of rejected transactions by id, and their rejection reasons.
    rejected: HashMap<UnminedTxId, State>,

    /// The set of spent out points by the verified transactions.
    spent_outpoints: HashSet<transparent::OutPoint>,

    /// The set of revealed Sprout nullifiers.
    sprout_nullifiers: HashSet<sprout::Nullifier>,

    /// The set of revealed Sapling nullifiers.
    sapling_nullifiers: HashSet<sapling::Nullifier>,

    /// The set of revealed Orchard nullifiers.
    orchard_nullifiers: HashSet<orchard::Nullifier>,
}

impl Storage {
    /// Insert a [`UnminedTx`] into the mempool.
    ///
    /// If its insertion results in evicting other transactions, they will be tracked
    /// as [`State::Excess`].
    pub fn insert(&mut self, tx: UnminedTx) -> Result<UnminedTxId, MempoolError> {
        let tx_id = tx.id;

        // First, check if we should reject this transaction.
        if self.rejected.contains_key(&tx.id) {
            return Err(match self.rejected.get(&tx.id).unwrap() {
                State::Invalid(e) => MempoolError::Invalid(e.clone()),
                State::Expired => MempoolError::Expired,
                State::Confirmed(block_hash) => MempoolError::InBlock(*block_hash),
                State::Excess => MempoolError::Excess,
                State::LowFee => MempoolError::LowFee,
                State::SpendConflict => MempoolError::SpendConflict,
            });
        }

        // If `tx` is already in the mempool, we don't change anything.
        //
        // Security: transactions must not get refreshed by new queries,
        // because that allows malicious peers to keep transactions live forever.
        if self.verified.contains(&tx) {
            return Err(MempoolError::InMempool);
        }

        // If `tx` spends an UTXO already spent by another transaction in the mempool or reveals a
        // nullifier already revealed by another transaction in the mempool, reject that
        // transaction.
        if self.check_spend_conflicts(&tx) {
            self.rejected.insert(tx.id, State::SpendConflict);
            return Err(MempoolError::Rejected);
        }

        // Then, we insert into the pool.
        self.verified.push_front(tx);

        // Once inserted, we evict transactions over the pool size limit in FIFO
        // order.
        if self.verified.len() > MEMPOOL_SIZE {
            for evicted_tx in self.verified.drain(MEMPOOL_SIZE..) {
                let _ = self.rejected.insert(evicted_tx.id, State::Excess);
            }

            assert_eq!(self.verified.len(), MEMPOOL_SIZE);
        }

        Ok(tx_id)
    }

    /// Returns `true` if a [`UnminedTx`] matching an [`UnminedTxId`] is in
    /// the mempool.
    pub fn contains(&self, txid: &UnminedTxId) -> bool {
        self.verified.iter().any(|tx| &tx.id == txid)
    }

    /// Remove a [`UnminedTx`] from the mempool via [`UnminedTxId`].  Returns
    /// whether the transaction was present.
    ///
    /// Removes from the 'verified' set, does not remove from the 'rejected'
    /// tracking set, if present. Maintains the order in which the other unmined
    /// transactions have been inserted into the mempool.
    pub fn remove(&mut self, txid: &UnminedTxId) -> Option<UnminedTx> {
        // If the txid exists in the verified set and is then deleted,
        // `retain()` removes it and returns `Some(UnminedTx)`. If it's not
        // present and nothing changes, returns `None`.

        match self.verified.iter().find(|tx| &tx.id == txid).cloned() {
            Some(tx) => {
                self.verified.retain(|tx| &tx.id != txid);
                self.remove_outputs(&tx);
                Some(tx)
            }
            None => None,
        }
    }

    /// Returns the set of [`UnminedTxId`]s in the mempool.
    pub fn tx_ids(&self) -> Vec<UnminedTxId> {
        self.verified.iter().map(|tx| tx.id).collect()
    }

    /// Returns the set of [`Transaction`]s matching ids in the mempool.
    pub fn transactions(&self, tx_ids: HashSet<UnminedTxId>) -> Vec<UnminedTx> {
        self.verified
            .iter()
            .filter(|tx| tx_ids.contains(&tx.id))
            .cloned()
            .collect()
    }

    /// Returns `true` if a [`UnminedTx`] matching an [`UnminedTxId`] is in
    /// the mempool rejected list.
    pub fn contains_rejected(&self, txid: &UnminedTxId) -> bool {
        self.rejected.contains_key(txid)
    }

    /// Returns the set of [`UnminedTxId`]s matching ids in the rejected list.
    pub fn rejected_transactions(&self, tx_ids: HashSet<UnminedTxId>) -> Vec<UnminedTxId> {
        tx_ids
            .into_iter()
            .filter(|tx| self.rejected.contains_key(tx))
            .collect()
    }

    /// Clears the whole mempool storage.
    pub fn clear(&mut self) {
        self.verified.clear();
        self.rejected.clear();
        self.spent_outpoints.clear();
        self.sprout_nullifiers.clear();
        self.sapling_nullifiers.clear();
        self.orchard_nullifiers.clear();
    }

    /// Removes the tracked transaction outputs from the mempool.
    fn remove_outputs(&mut self, unmined_tx: &UnminedTx) {
        let tx = &unmined_tx.transaction;

        let spent_outpoints = tx.spent_outpoints().map(Cow::Owned);
        let sprout_nullifiers = tx.sprout_nullifiers().map(Cow::Borrowed);
        let sapling_nullifiers = tx.sapling_nullifiers().map(Cow::Borrowed);
        let orchard_nullifiers = tx.orchard_nullifiers().map(Cow::Borrowed);

        Self::remove_from_set(&mut self.spent_outpoints, spent_outpoints);
        Self::remove_from_set(&mut self.sprout_nullifiers, sprout_nullifiers);
        Self::remove_from_set(&mut self.sapling_nullifiers, sapling_nullifiers);
        Self::remove_from_set(&mut self.orchard_nullifiers, orchard_nullifiers);
    }

    /// Checks if the `unmined_tx` transaction has spend conflicts with another transaction in the
    /// mempool.
    ///
    /// Two transactions have a spend conflict if they spent the same UTXO or if they reveal the
    /// same nullifier.
    fn check_spend_conflicts(&mut self, unmined_tx: &UnminedTx) -> bool {
        let tx = &unmined_tx.transaction;

        let new_spent_outpoints = tx.spent_outpoints().map(Cow::Owned);
        let new_sprout_nullifiers = tx.sprout_nullifiers().map(Cow::Borrowed);
        let new_sapling_nullifiers = tx.sapling_nullifiers().map(Cow::Borrowed);
        let new_orchard_nullifiers = tx.orchard_nullifiers().map(Cow::Borrowed);

        Self::has_spend_conflicts(&mut self.spent_outpoints, new_spent_outpoints)
            || Self::has_spend_conflicts(&mut self.sprout_nullifiers, new_sprout_nullifiers)
            || Self::has_spend_conflicts(&mut self.sapling_nullifiers, new_sapling_nullifiers)
            || Self::has_spend_conflicts(&mut self.orchard_nullifiers, new_orchard_nullifiers)
    }

    /// Checks if the `new_outputs` from a transaction conflict with the `existing_outputs` from
    /// the transactions already in the mempool.
    ///
    /// Each output in the `new_outputs` list should be wrapped in a [`Cow`]. This allows this
    /// generic method to support both borrowed and owned items.
    fn has_spend_conflicts<'tx, Output>(
        existing_outputs: &mut HashSet<Output>,
        new_outputs: impl Iterator<Item = Cow<'tx, Output>>,
    ) -> bool
    where
        Output: Clone + Eq + Hash + 'tx,
    {
        let mut inserted_outputs = Vec::with_capacity(new_outputs.size_hint().1.unwrap_or(0));

        for new_output in new_outputs {
            if existing_outputs.insert(new_output.clone().into_owned()) {
                inserted_outputs.push(new_output);
            } else {
                Self::remove_from_set(existing_outputs, inserted_outputs);
                return true;
            }
        }

        false
    }

    /// Removes some items from a [`HashSet`].
    ///
    /// Each item in the list of `items` should be wrapped in a [`Cow`]. This allows this generic
    /// method to support both borrowed and owned items.
    fn remove_from_set<'t, T>(set: &mut HashSet<T>, items: impl IntoIterator<Item = Cow<'t, T>>)
    where
        T: Clone + Eq + Hash + 't,
    {
        for item in items {
            set.remove(&item);
        }
    }
}
