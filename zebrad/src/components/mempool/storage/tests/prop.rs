use std::fmt::Debug;

use proptest::prelude::*;
use proptest_derive::Arbitrary;

use zebra_chain::{
    at_least_one, orchard,
    primitives::Groth16Proof,
    sapling,
    transaction::{self, Transaction, UnminedTx},
    transparent, LedgerState,
};

use super::super::{MempoolError, Storage};

proptest! {
    #[test]
    fn conflicting_transactions_are_rejected(input in any::<SpendConflictTestInput>()) {
        let mut storage = Storage::default();

        let (first_transaction, second_transaction) = input.conflicting_transactions();
        let input_permutations = vec![
            (first_transaction.clone(), second_transaction.clone()),
            (second_transaction, first_transaction),
        ];

        for (transaction_to_accept, transaction_to_reject) in input_permutations {
            let id_to_accept = transaction_to_accept.id;
            let id_to_reject = transaction_to_reject.id;

            assert_eq!(
                storage.insert(transaction_to_accept),
                Ok(id_to_accept)
            );

            assert_eq!(
                storage.insert(transaction_to_reject),
                Err(MempoolError::Rejected)
            );

            assert!(storage.contains_rejected(&id_to_reject));

            storage.clear();
        }
    }
}

#[derive(Arbitrary, Debug)]
enum SpendConflictTestInput {
    V4 {
        #[proptest(strategy = "Transaction::v4_strategy(LedgerState::default())")]
        first: Transaction,

        #[proptest(strategy = "Transaction::v4_strategy(LedgerState::default())")]
        second: Transaction,

        conflict: SpendConflictForTransactionV4,
    },

    V5 {
        #[proptest(strategy = "Transaction::v5_strategy(LedgerState::default())")]
        first: Transaction,

        #[proptest(strategy = "Transaction::v5_strategy(LedgerState::default())")]
        second: Transaction,

        conflict: SpendConflictForTransactionV5,
    },
}

impl SpendConflictTestInput {
    pub fn conflicting_transactions(self) -> (UnminedTx, UnminedTx) {
        let (first, second) = match self {
            SpendConflictTestInput::V4 {
                mut first,
                mut second,
                conflict,
            } => {
                conflict.clone().apply_to(&mut first);
                conflict.apply_to(&mut second);

                (first, second)
            }
            SpendConflictTestInput::V5 {
                mut first,
                mut second,
                conflict,
            } => {
                conflict.clone().apply_to(&mut first);
                conflict.apply_to(&mut second);

                (first, second)
            }
        };

        (first.into(), second.into())
    }
}

#[derive(Arbitrary, Clone, Debug)]
enum SpendConflictForTransactionV4 {
    Transparent(TransparentSpendConflict),
    Sprout(SproutSpendConflict),
    Sapling(SaplingSpendConflict<sapling::PerSpendAnchor>),
}

#[derive(Arbitrary, Clone, Debug)]
enum SpendConflictForTransactionV5 {
    Transparent(TransparentSpendConflict),
    Sapling(SaplingSpendConflict<sapling::SharedAnchor>),
    Orchard(OrchardSpendConflict),
}

#[derive(Arbitrary, Clone, Debug)]
struct TransparentSpendConflict {
    new_input: transparent::Input,
}

#[derive(Arbitrary, Clone, Debug)]
struct SproutSpendConflict {
    new_joinsplit_data: transaction::JoinSplitData<Groth16Proof>,
}

#[derive(Clone, Debug)]
struct SaplingSpendConflict<A: sapling::AnchorVariant + Clone> {
    new_spend: sapling::Spend<A>,
    new_shared_anchor: A::Shared,
    fallback_shielded_data: sapling::ShieldedData<A>,
}

#[derive(Arbitrary, Clone, Debug)]
struct OrchardSpendConflict {
    new_shielded_data: orchard::ShieldedData,
}

impl SpendConflictForTransactionV4 {
    pub fn apply_to(self, transaction_v4: &mut Transaction) {
        let (inputs, joinsplit_data, sapling_shielded_data) = match transaction_v4 {
            Transaction::V4 {
                inputs,
                joinsplit_data,
                sapling_shielded_data,
                ..
            } => (inputs, joinsplit_data, sapling_shielded_data),
            _ => unreachable!("incorrect transaction version generated for test"),
        };

        use SpendConflictForTransactionV4::*;
        match self {
            Transparent(transparent_conflict) => transparent_conflict.apply_to(inputs),
            Sprout(sprout_conflict) => sprout_conflict.apply_to(joinsplit_data),
            Sapling(sapling_conflict) => sapling_conflict.apply_to(sapling_shielded_data),
        }
    }
}

impl SpendConflictForTransactionV5 {
    pub fn apply_to(self, transaction_v5: &mut Transaction) {
        let (inputs, sapling_shielded_data, orchard_shielded_data) = match transaction_v5 {
            Transaction::V5 {
                inputs,
                sapling_shielded_data,
                orchard_shielded_data,
                ..
            } => (inputs, sapling_shielded_data, orchard_shielded_data),
            _ => unreachable!("incorrect transaction version generated for test"),
        };

        use SpendConflictForTransactionV5::*;
        match self {
            Transparent(transparent_conflict) => transparent_conflict.apply_to(inputs),
            Sapling(sapling_conflict) => sapling_conflict.apply_to(sapling_shielded_data),
            Orchard(orchard_conflict) => orchard_conflict.apply_to(orchard_shielded_data),
        }
    }
}

impl TransparentSpendConflict {
    pub fn apply_to(self, inputs: &mut Vec<transparent::Input>) {
        inputs.push(self.new_input);
    }
}

impl SproutSpendConflict {
    pub fn apply_to(self, joinsplit_data: &mut Option<transaction::JoinSplitData<Groth16Proof>>) {
        if let Some(existing_joinsplit_data) = joinsplit_data.as_mut() {
            existing_joinsplit_data.first.nullifiers[0] =
                self.new_joinsplit_data.first.nullifiers[0];
        } else {
            *joinsplit_data = Some(self.new_joinsplit_data);
        }
    }
}

impl<A> Arbitrary for SaplingSpendConflict<A>
where
    A: sapling::AnchorVariant + Clone + Debug + 'static,
    A::Shared: Arbitrary,
    sapling::Spend<A>: Arbitrary,
    sapling::TransferData<A>: Arbitrary,
{
    type Parameters = ();

    fn arbitrary_with(_: Self::Parameters) -> Self::Strategy {
        any::<(sapling::Spend<A>, A::Shared, sapling::ShieldedData<A>)>()
            .prop_map(|(new_spend, new_shared_anchor, fallback_shielded_data)| {
                SaplingSpendConflict {
                    new_spend,
                    new_shared_anchor,
                    fallback_shielded_data,
                }
            })
            .boxed()
    }

    type Strategy = BoxedStrategy<Self>;
}

impl<A: sapling::AnchorVariant + Clone> SaplingSpendConflict<A> {
    pub fn apply_to(self, sapling_shielded_data: &mut Option<sapling::ShieldedData<A>>) {
        use sapling::TransferData::*;

        let shielded_data = sapling_shielded_data.get_or_insert(self.fallback_shielded_data);

        match &mut shielded_data.transfers {
            SpendsAndMaybeOutputs { ref mut spends, .. } => spends.push(self.new_spend),
            JustOutputs { ref mut outputs } => {
                let new_outputs = outputs.clone();

                shielded_data.transfers = SpendsAndMaybeOutputs {
                    shared_anchor: self.new_shared_anchor,
                    spends: at_least_one![self.new_spend],
                    maybe_outputs: new_outputs.into_vec(),
                };
            }
        }
    }
}

impl OrchardSpendConflict {
    pub fn apply_to(self, orchard_shielded_data: &mut Option<orchard::ShieldedData>) {
        if let Some(shielded_data) = orchard_shielded_data.as_mut() {
            shielded_data.actions.first_mut().action.nullifier =
                self.new_shielded_data.actions.first().action.nullifier;
        } else {
            *orchard_shielded_data = Some(self.new_shielded_data);
        }
    }
}
