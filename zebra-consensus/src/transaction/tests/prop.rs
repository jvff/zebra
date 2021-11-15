use std::{collections::HashMap, sync::Arc};

use proptest::prelude::*;
use tower::ServiceExt;

use zebra_chain::{
    parameters::Network,
    transaction::{LockTime, Transaction},
    transparent,
};

use crate::{error::TransactionError, script, transaction};

proptest! {
    #[test]
    fn zero_lock_time_is_always_unlocked(
        network in any::<Network>(),
        (transaction, known_utxos) in Transaction::valid_transparent_transfer_strategy(),
    ) {
        zebra_test::init();

        transaction.set_lock_time(LockTime::unlocked());

        let transaction_id = transaction.unmined_id();

        let result = validate(transaction, known_utxos, network);

        prop_assert!(result.is_ok());
        prop_assert_eq!(result.unwrap().tx_id(), transaction_id);
    }

    // #[test]
    // fn lock_time_is_ignored_because_of_sequence_numbers() {
        // todo!();
    // }

    // #[test]
    // fn transaction_is_rejected_based_on_lock_height() {
        // todo!();
    // }

    // #[test]
    // fn transaction_is_rejected_based_on_lock_time() {
        // todo!();
    // }

    // #[test]
    // fn transaction_with_lock_height_is_accepted() {
        // todo!();
    // }

    // #[test]
    // fn transaction_with_lock_time_is_accepted() {
        // todo!();
    // }
}

fn validate(
    transaction: Transaction,
    known_utxos: HashMap<transparent::OutPoint, transparent::OrderedUtxo>,
    network: Network,
) -> Result<transaction::Response, TransactionError> {
    zebra_test::RUNTIME.block_on(async {
        // Initialize the verifier
        let state_service =
            tower::service_fn(|_| async { unreachable!("State service should not be called") });
        let script_verifier = script::Verifier::new(state_service);
        let verifier = transaction::Verifier::new(network, script_verifier);

        // Test the transaction verifier
        verifier
            .clone()
            .oneshot(transaction::Request::Block {
                transaction: Arc::new(transaction),
                known_utxos: Arc::new(known_utxos),
                height,
            })
            .await
    })
}
