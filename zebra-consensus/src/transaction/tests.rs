use zebra_chain::{
    block::{Block, Height},
    parameters::Network,
    serialization::ZcashDeserializeInto,
    transaction::{arbitrary::transaction_to_fake_v5, Transaction},
};

use super::check;

use crate::error::TransactionError;
use color_eyre::eyre::Report;

#[test]
fn v5_fake_transactions() -> Result<(), Report> {
    zebra_test::init();

    let networks = vec![
        (Network::Mainnet, zebra_test::vectors::MAINNET_BLOCKS.iter()),
        (Network::Testnet, zebra_test::vectors::TESTNET_BLOCKS.iter()),
    ];

    for (network, blocks) in networks {
        for transaction in v5_fake_transactions_for_network(network, blocks) {
            match check::has_inputs_and_outputs(&transaction) {
                Ok(()) => (),
                Err(TransactionError::NoInputs) | Err(TransactionError::NoOutputs) => (),
                Err(_) => panic!("error must be NoInputs or NoOutputs"),
            };

            // make sure there are no joinsplits nor spends in coinbase
            check::coinbase_tx_no_prevout_joinsplit_spend(&transaction)?;

            // validate the sapling shielded data
            match transaction {
                Transaction::V5 {
                    sapling_shielded_data,
                    ..
                } => {
                    if let Some(s) = sapling_shielded_data {
                        check::sapling_balances_match(&s)?;

                        for spend in s.spends_per_anchor() {
                            check::spend_cv_rk_not_small_order(&spend)?
                        }
                        for output in s.outputs() {
                            check::output_cv_epk_not_small_order(&output)?;
                        }
                    }
                }
                _ => panic!("we should have no tx other than 5"),
            }
        }
    }

    Ok(())
}

fn v5_fake_transactions_for_network<'b>(
    network: Network,
    blocks: impl DoubleEndedIterator<Item = (&'b u32, &'b &'static [u8])> + 'b,
) -> impl DoubleEndedIterator<Item = Transaction> + 'b {
    blocks.flat_map(move |(height, original_bytes)| {
        let original_block = original_bytes
            .zcash_deserialize_into::<Block>()
            .expect("block is structurally valid");

        original_block
            .transactions
            .into_iter()
            .map(move |transaction| transaction_to_fake_v5(&*transaction, network, Height(*height)))
            .map(Transaction::from)
    })
}
