use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    hash::Hash,
};

use thiserror::Error;

use zebra_chain::{
    orchard, sapling, sprout,
    transaction::{self, UnminedTx, UnminedTxId},
    transparent,
};

use super::MempoolError;

#[cfg(any(test, feature = "proptest-impl"))]
use proptest_derive::Arbitrary;

#[cfg(test)]
pub mod tests;

const MEMPOOL_SIZE: usize = 2;

#[derive(Error, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "proptest-impl"), derive(Arbitrary))]
#[allow(dead_code)]
pub enum StorageRejectionError {
    #[error(
        "transaction rejected because another transaction in the mempool has already spent some of \
        its inputs"
    )]
    SpendConflict,

    #[error("best chain tip has reached transaction expiry height")]
    Expired,

    /// Otherwise valid transaction removed from mempool due to ZIP-401 random eviction.
    ///
    /// Consensus rule:
    /// > The txid (rather than the wtxid ...) is used even for version 5 transactions
    ///
    /// https://zips.z.cash/zip-0401#specification
    #[error("transaction evicted from the mempool due to ZIP-401 denial of service limits")]
    RandomlyEvicted,

    #[error("transaction did not pass consensus validation")]
    FailedVerification(#[from] zebra_consensus::error::TransactionError),
}

#[derive(Default)]
pub struct Storage {
    /// The set of verified transactions in the mempool. This is a
    /// cache of size [`MEMPOOL_SIZE`].
    verified: VecDeque<UnminedTx>,

    /// The set of rejected transactions by id, and their rejection reasons.
    rejected: HashMap<UnminedTxId, StorageRejectionError>,

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
    /// as [`StorageRejectionError::RandomlyEvicted`].
    pub fn insert(&mut self, tx: UnminedTx) -> Result<UnminedTxId, MempoolError> {
        let tx_id = tx.id;

        // First, check if we have a cached rejection for this transaction.
        if let Some(error) = self.rejection_error(&tx.id) {
            return Err(error.into());
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
            self.rejected
                .insert(tx.id, StorageRejectionError::SpendConflict);
            return Err(StorageRejectionError::SpendConflict.into());
        }

        // Then, we insert into the pool.
        self.verified.push_front(tx);

        // Once inserted, we evict transactions over the pool size limit in FIFO
        // order.
        //
        // TODO: use random weighted eviction as specified in ZIP-401 (#2780)
        if self.verified.len() > MEMPOOL_SIZE {
            for evicted_tx in self.verified.drain(MEMPOOL_SIZE..) {
                let _ = self
                    .rejected
                    .insert(evicted_tx.id, StorageRejectionError::RandomlyEvicted);
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

    /// Remove [`UnminedTx`]es from the mempool via exact [`UnminedTxId`].
    ///
    /// For v5 transactions, transactions are matched by WTXID, using both the:
    /// - non-malleable transaction ID, and
    /// - authorizing data hash.
    ///
    /// This matches the exact transaction, with identical blockchain effects, signatures, and proofs.
    ///
    /// Returns the number of transactions which were removed.
    ///
    /// Removes from the 'verified' set, if present.
    /// Maintains the order in which the other unmined transactions have been inserted into the mempool.
    ///
    /// Does not add or remove from the 'rejected' tracking set.
    #[allow(dead_code)]
    pub fn remove_exact(&mut self, exact_wtxids: &HashSet<UnminedTxId>) -> usize {
        let original_size = self.verified.len();

        self.verified.retain(|tx| !exact_wtxids.contains(&tx.id));

        original_size - self.verified.len()
    }

    /// Remove [`UnminedTx`]es from the mempool via non-malleable [`transaction::Hash`].
    ///
    /// For v5 transactions, transactions are matched by TXID,
    /// using only the non-malleable transaction ID.
    /// This matches any transaction with the same effect on the blockchain state,
    /// even if its signatures and proofs are different.
    ///
    /// Returns the number of transactions which were removed.
    ///
    /// Removes from the 'verified' set, if present.
    /// Maintains the order in which the other unmined transactions have been inserted into the mempool.
    ///
    /// Does not add or remove from the 'rejected' tracking set.
    pub fn remove_same_effects(&mut self, mined_ids: &HashSet<transaction::Hash>) -> usize {
        let original_size = self.verified.len();

        self.verified
            .retain(|tx| !mined_ids.contains(&tx.id.mined_id()));

        original_size - self.verified.len()
    }

    /// Returns the set of [`UnminedTxId`]s in the mempool.
    pub fn tx_ids(&self) -> Vec<UnminedTxId> {
        self.verified.iter().map(|tx| tx.id).collect()
    }

    /// Returns the set of [`Transaction`][transaction::Transaction]s matching `tx_ids` in the
    /// mempool.
    pub fn transactions(&self, tx_ids: HashSet<UnminedTxId>) -> Vec<UnminedTx> {
        self.verified
            .iter()
            .filter(|tx| tx_ids.contains(&tx.id))
            .cloned()
            .collect()
    }

    /// Returns the set of [`Transaction`][transaction::Transaction]s in the mempool.
    pub fn transactions_all(&self) -> Vec<UnminedTx> {
        self.verified.iter().cloned().collect()
    }

    /// Returns `true` if a [`UnminedTx`] matching an [`UnminedTxId`] is in
    /// the mempool rejected list.
    #[allow(dead_code)]
    pub fn contains_rejected(&self, txid: &UnminedTxId) -> bool {
        self.rejected.contains_key(txid)
    }

    /// Returns `true` if a [`UnminedTx`] matching an [`UnminedTxId`] is in
    /// the mempool rejected list.
    pub fn rejection_error(&self, txid: &UnminedTxId) -> Option<StorageRejectionError> {
        self.rejected.get(txid).cloned()
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
        for new_output in new_outputs {
            if !existing_outputs.insert(new_output.into_owned()) {
                return true;
            }
        }

        false
    }
}
