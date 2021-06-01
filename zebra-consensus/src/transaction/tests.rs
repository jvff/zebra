use std::convert::TryFrom;

use zebra_chain::{
    amount::Amount,
    block::{Block, Height},
    orchard,
    parameters::{Network, NetworkUpgrade},
    primitives::{redpallas::Signature, Halo2Proof},
    serialization::{AtLeastOne, ZcashDeserializeInto},
    transaction::{arbitrary::transaction_to_fake_v5, LockTime, Transaction},
};

use crate::error::TransactionError::*;
use color_eyre::eyre::Report;

#[test]
fn transaction_with_orchard_actions_has_inputs_and_outputs() {
    let dummy_action = orchard::AuthorizedAction {
        action: ,
        spend_auth_sig: Signature::from([0u8; 64]);
    };

    let dummy_orchard_shielded_data = orchard::ShieldedData {
        flags: orchard::Flags::ENABLE_SPENDS,
        value_balance: Amount::try_from(0).expect("invalid transaction amount"),
        shared_anchor: orchard::tree::Root::default(),
        proof: Halo2Proof(vec![]),
        actions: AtLeastOne::try_from(vec![action]).expect("empty action list"),
        binding_sig: Signature::from([0u8; 64]),
    };

    let transaction = Transaction::V5 {
        network_upgrade: NetworkUpgrade::Nu5,
        lock_time: LockTime::Height(Height(0)),
        expiry_height: Height(u32::MAX),
        inputs: vec![],
        outputs: vec![],
        sapling_shielded_data: None,
        orchard_shielded_data: Some(dummy_orchard_shielded_data),
    };
}

#[test]
fn v5_fake_transactions() -> Result<(), Report> {
    zebra_test::init();

    v5_fake_transactions_for_network(Network::Mainnet)?;
    v5_fake_transactions_for_network(Network::Testnet)?;

    Ok(())
}

fn v5_fake_transactions_for_network(network: Network) -> Result<(), Report> {
    zebra_test::init();

    // get all the blocks we have available
    let block_iter = match network {
        Network::Mainnet => zebra_test::vectors::MAINNET_BLOCKS.iter(),
        Network::Testnet => zebra_test::vectors::TESTNET_BLOCKS.iter(),
    };

    for (height, original_bytes) in block_iter {
        let original_block = original_bytes
            .zcash_deserialize_into::<Block>()
            .expect("block is structurally valid");

        // convert all transactions from the block to V5
        let transactions: Vec<Transaction> = original_block
            .transactions
            .iter()
            .map(AsRef::as_ref)
            .map(|t| transaction_to_fake_v5(t, network, Height(*height)))
            .map(Into::into)
            .collect();

        // after the conversion some transactions end up with no inputs nor outputs.
        for transaction in transactions {
            match super::check::has_inputs_and_outputs(&transaction) {
                Err(e) => {
                    if e != NoInputs && e != NoOutputs {
                        panic!("error must be NoInputs or NoOutputs")
                    }
                }
                Ok(()) => (),
            };

            // make sure there are no joinsplits nor spends in coinbase
            super::check::coinbase_tx_no_prevout_joinsplit_spend(&transaction)?;

            // validate the sapling shielded data
            match transaction {
                Transaction::V5 {
                    sapling_shielded_data,
                    ..
                } => {
                    if let Some(s) = sapling_shielded_data {
                        super::check::sapling_balances_match(&s)?;

                        for spend in s.spends_per_anchor() {
                            super::check::spend_cv_rk_not_small_order(&spend)?
                        }
                        for output in s.outputs() {
                            super::check::output_cv_epk_not_small_order(&output)?;
                        }
                    }
                }
                _ => panic!("we should have no tx other than 5"),
            }
        }
    }
    Ok(())
}
