use std::{convert::TryFrom, ops::RangeBounds};

use super::*;

use zebra_chain::{
    amount::Amount,
    block::{self, Block},
    parameters::{Network, NetworkUpgrade},
    serialization::ZcashDeserializeInto,
    transaction::{LockTime, UnminedTx},
    transparent,
};

use color_eyre::eyre::Result;

#[test]
fn mempool_storage_crud_mainnet() {
    zebra_test::init();

    let network = Network::Mainnet;

    // Create an empty storage instance
    let mut storage: Storage = Default::default();

    // Get one (1) unmined transaction
    let unmined_tx = unmined_transactions_in_blocks(.., network)
        .next()
        .expect("at least one unmined transaction");

    // Insert unmined tx into the mempool.
    let _ = storage.insert(unmined_tx.clone());

    // Check that it is in the mempool, and not rejected.
    assert!(storage.contains(&unmined_tx.id));

    // Remove tx
    let _ = storage.remove(&unmined_tx.id);

    // Check that it is /not/ in the mempool.
    assert!(!storage.contains(&unmined_tx.id));
}

#[test]
fn mempool_storage_basic() -> Result<()> {
    zebra_test::init();

    mempool_storage_basic_for_network(Network::Mainnet)?;
    mempool_storage_basic_for_network(Network::Testnet)?;

    Ok(())
}

fn mempool_storage_basic_for_network(network: Network) -> Result<()> {
    // Create an empty storage
    let mut storage: Storage = Default::default();

    // Get transactions from the first 10 blocks of the Zcash blockchain
    let unmined_transactions: Vec<_> = unmined_transactions_in_blocks(..=10, network).collect();
    let total_transactions = unmined_transactions.len();

    // Insert them all to the storage
    for unmined_transaction in unmined_transactions.clone() {
        storage.insert(unmined_transaction)?;
    }

    // Separate transactions into the ones expected to be in the mempool and those expected to be
    // rejected.
    let rejected_transaction_count = total_transactions - MEMPOOL_SIZE;
    let expected_to_be_rejected = &unmined_transactions[..rejected_transaction_count];
    let expected_in_mempool = &unmined_transactions[rejected_transaction_count..];

    // Only MEMPOOL_SIZE should land in verified
    assert_eq!(storage.verified.len(), MEMPOOL_SIZE);

    // The rest of the transactions will be in rejected
    assert_eq!(storage.rejected.len(), rejected_transaction_count);

    // Make sure the last MEMPOOL_SIZE transactions we sent are in the verified
    for tx in expected_in_mempool {
        assert!(storage.contains(&tx.id));
    }

    // Anything greater should not be in the verified
    for tx in expected_to_be_rejected {
        assert!(!storage.contains(&tx.id));
    }

    // Query all the ids we have for rejected, get back `total - MEMPOOL_SIZE`
    let all_ids: HashSet<UnminedTxId> = unmined_transactions.iter().map(|tx| tx.id).collect();

    // Convert response to a `HashSet` as we need a fixed order to compare.
    let rejected_response: HashSet<UnminedTxId> =
        storage.rejected_transactions(all_ids).into_iter().collect();

    let rejected_ids = expected_to_be_rejected.iter().map(|tx| tx.id).collect();

    assert_eq!(rejected_response, rejected_ids);

    // Use `contains_rejected` to make sure the first id stored is now rejected
    assert!(storage.contains_rejected(&expected_to_be_rejected[0].id));
    // Use `contains_rejected` to make sure the last id stored is not rejected
    assert!(!storage.contains_rejected(&expected_in_mempool[0].id));

    Ok(())
}

#[test]
fn conflicting_transactions_are_rejected() {
    let mut storage = Storage::default();

    let mut inputs = inputs_from_blocks(.., Network::Mainnet);

    let shared_input = inputs
        .next()
        .expect("At least one input from unmined blocks");
    let first_transaction_input = inputs
        .next()
        .expect("At least two inputs from unmined blocks");
    let second_transaction_input = inputs
        .next()
        .expect("At least three inputs from unmined blocks");

    assert_only_one_transaction_is_inserted(
        &mut storage,
        mock_transparent_transaction(vec![shared_input.clone(), first_transaction_input]),
        mock_transparent_transaction(vec![shared_input, second_transaction_input]),
    );
}

fn assert_only_one_transaction_is_inserted(
    storage: &mut Storage,
    first_transaction: UnminedTx,
    second_transaction: UnminedTx,
) {
    let first_transaction_id = first_transaction.id;
    let second_transaction_id = second_transaction.id;

    // Test inserting the first then the second
    assert_eq!(
        storage.insert(first_transaction.clone()),
        Ok(first_transaction_id)
    );
    assert_eq!(
        storage.insert(second_transaction.clone()),
        Err(MempoolError::Rejected)
    );
    assert!(storage.contains_rejected(&second_transaction_id));

    storage.clear();

    // Test inserting the second then the first
    assert_eq!(
        storage.insert(second_transaction),
        Ok(second_transaction_id)
    );
    assert_eq!(
        storage.insert(first_transaction),
        Err(MempoolError::Rejected)
    );
    assert!(storage.contains_rejected(&first_transaction_id));
}

pub fn unmined_transactions_in_blocks(
    block_height_range: impl RangeBounds<u32>,
    network: Network,
) -> impl DoubleEndedIterator<Item = UnminedTx> {
    let blocks = match network {
        Network::Mainnet => zebra_test::vectors::MAINNET_BLOCKS.iter(),
        Network::Testnet => zebra_test::vectors::TESTNET_BLOCKS.iter(),
    };

    // Deserialize the blocks that are selected based on the specified `block_height_range`.
    let selected_blocks = blocks
        .filter(move |(&height, _)| block_height_range.contains(&height))
        .map(|(_, block)| {
            block
                .zcash_deserialize_into::<Block>()
                .expect("block test vector is structurally valid")
        });

    // Extract the transactions from the blocks and warp each one as an unmined transaction.
    selected_blocks
        .flat_map(|block| block.transactions)
        .map(UnminedTx::from)
}

fn inputs_from_blocks(
    block_height_range: impl RangeBounds<u32>,
    network: Network,
) -> impl DoubleEndedIterator<Item = transparent::Input> {
    // Create an unlock script that allows any other transaction to spend the UTXO. This is a
    // script with a single opcode that accepts the transaction (pushes true on the stack).
    let accepting_script = transparent::Script::new(&[1, 1]);

    unmined_transactions_in_blocks(block_height_range, network)
        // Isolate the `Arc<Transaction>`
        .map(|unmined_transaction| unmined_transaction.transaction)
        // Filter out coinbase transactions
        .filter(|transaction| !transaction.has_any_coinbase_inputs())
        // Build an outpoint for every UTXO created by a transaction
        .flat_map(|transaction| {
            let output_count = transaction.outputs().len() as u32;
            let transaction_hash = transaction.hash();

            (0..output_count).map(move |index| transparent::OutPoint {
                hash: transaction_hash,
                index,
            })
        })
        // Transform the outpoint into an input
        .map(move |outpoint| transparent::Input::PrevOut {
            outpoint,
            unlock_script: accepting_script.clone(),
            sequence: 0xffffffff,
        })
}

fn mock_transparent_transaction(inputs: Vec<transparent::Input>) -> UnminedTx {
    // A script with a single opcode that accepts the transaction (pushes true on the stack)
    let accepting_script = transparent::Script::new(&[1, 1]);

    let output = transparent::Output {
        value: Amount::try_from(1).expect("1 is non-negative"),
        lock_script: accepting_script,
    };

    UnminedTx::from(Transaction::V5 {
        network_upgrade: NetworkUpgrade::Nu5,
        lock_time: LockTime::min_lock_time(),
        expiry_height: block::Height::MAX,
        inputs,
        outputs: vec![output],
        sapling_shielded_data: None,
        orchard_shielded_data: None,
    })
}
