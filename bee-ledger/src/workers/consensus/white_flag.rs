// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{
    types::{BalanceDiffs, ConsumedOutput, CreatedOutput},
    workers::{
        consensus::{merkle_hasher::MerkleHasher, metadata::WhiteFlagMetadata},
        error::Error,
        storage::{self, StorageBackend},
    },
};

use bee_ledger_types::types::ConflictReason;
use bee_message::{
    address::Address,
    input::Input,
    output::{dust_outputs_max, Output, OutputId, DUST_THRESHOLD},
    payload::{
        transaction::{Essence, RegularEssence, TransactionId, TransactionPayload},
        Payload,
    },
    unlock::{UnlockBlock, UnlockBlocks},
    Message, MessageId,
};
use bee_tangle::MsTangle;

use crypto::hashes::blake2b::Blake2b256;

use std::collections::{HashMap, HashSet};

fn verify_signature(address: &Address, unlock_blocks: &UnlockBlocks, index: usize, essence_hash: &[u8; 32]) -> bool {
    match unlock_blocks.get(index) {
        Some(UnlockBlock::Signature(signature)) => address.verify(essence_hash, signature).is_ok(),
        _ => false,
    }
}

async fn apply_regular_essence<B: StorageBackend>(
    storage: &B,
    message_id: &MessageId,
    transaction_id: &TransactionId,
    essence: &RegularEssence,
    unlock_blocks: &UnlockBlocks,
    metadata: &mut WhiteFlagMetadata,
) -> Result<ConflictReason, Error> {
    let mut consumed_outputs = HashMap::with_capacity(essence.inputs().len());
    let mut balance_diffs = BalanceDiffs::new();
    let mut consumed_amount: u64 = 0;
    let mut created_amount: u64 = 0;

    for (index, input) in essence.inputs().iter().enumerate() {
        let (output_id, consumed_output) = match input {
            Input::Utxo(input) => {
                let output_id = input.output_id();

                if metadata.consumed_outputs.contains_key(output_id) {
                    return Ok(ConflictReason::InputUtxoAlreadySpentInThisMilestone);
                }

                if let Some(output) = metadata.created_outputs.get(output_id).cloned() {
                    (output_id, output)
                } else if let Some(output) = storage::fetch_output(storage, output_id).await? {
                    if !storage::is_output_unspent(storage, output_id).await? {
                        return Ok(ConflictReason::InputUtxoAlreadySpent);
                    }
                    (output_id, output)
                } else {
                    return Ok(ConflictReason::InputUtxoNotFound);
                }
            }
            input => {
                return Err(Error::UnsupportedInputKind(input.kind()));
            }
        };

        let essence_hash = Essence::from(essence.clone()).hash();

        match consumed_output.inner() {
            Output::SignatureLockedSingle(output) => {
                consumed_amount = consumed_amount
                    .checked_add(output.amount())
                    .ok_or_else(|| Error::ConsumedAmountOverflow(consumed_amount as u128 + output.amount() as u128))?;
                balance_diffs.amount_sub(*output.address(), output.amount())?;
                if output.amount() < DUST_THRESHOLD {
                    balance_diffs.dust_outputs_dec(*output.address())?;
                }

                if !verify_signature(output.address(), unlock_blocks, index, &essence_hash) {
                    return Ok(ConflictReason::InvalidSignature);
                }
            }
            Output::SignatureLockedDustAllowance(output) => {
                consumed_amount = consumed_amount
                    .checked_add(output.amount())
                    .ok_or_else(|| Error::ConsumedAmountOverflow(consumed_amount as u128 + output.amount() as u128))?;
                balance_diffs.amount_sub(*output.address(), output.amount())?;
                balance_diffs.dust_allowance_sub(*output.address(), output.amount())?;

                if !verify_signature(output.address(), unlock_blocks, index, &essence_hash) {
                    return Ok(ConflictReason::InvalidSignature);
                }
            }
            output => return Err(Error::UnsupportedOutputKind(output.kind())),
        }

        consumed_outputs.insert(*output_id, consumed_output);
    }

    for created_output in essence.outputs() {
        match created_output {
            Output::SignatureLockedSingle(output) => {
                created_amount = created_amount
                    .checked_add(output.amount())
                    .ok_or_else(|| Error::CreatedAmountOverflow(created_amount as u128 + output.amount() as u128))?;
                balance_diffs.amount_add(*output.address(), output.amount())?;
                if output.amount() < DUST_THRESHOLD {
                    balance_diffs.dust_outputs_inc(*output.address())?;
                }
            }
            Output::SignatureLockedDustAllowance(output) => {
                created_amount = created_amount
                    .checked_add(output.amount())
                    .ok_or_else(|| Error::CreatedAmountOverflow(created_amount as u128 + output.amount() as u128))?;
                balance_diffs.amount_add(*output.address(), output.amount())?;
                balance_diffs.dust_allowance_add(*output.address(), output.amount())?;
            }
            output => return Err(Error::UnsupportedOutputKind(output.kind())),
        }
    }

    if created_amount != consumed_amount {
        return Ok(ConflictReason::InputOutputSumMismatch);
    }

    for (address, diff) in balance_diffs.iter() {
        if diff.is_dust_mutating() {
            let mut balance = storage::fetch_balance_or_default(storage, &address)
                .await?
                .apply_diff(diff)?;

            if let Some(diff) = metadata.balance_diffs.get(&address) {
                balance = balance.apply_diff(diff)?;
            }

            if balance.dust_outputs() > dust_outputs_max(balance.dust_allowance()) {
                return Ok(ConflictReason::InvalidDustAllowance);
            }
        }
    }

    for (output_id, created_output) in consumed_outputs {
        metadata.consumed_outputs.insert(
            output_id,
            (created_output, ConsumedOutput::new(*transaction_id, metadata.index)),
        );
    }

    for (index, output) in essence.outputs().iter().enumerate() {
        metadata.created_outputs.insert(
            OutputId::new(*transaction_id, index as u16)?,
            CreatedOutput::new(*message_id, output.clone()),
        );
    }

    metadata.balance_diffs.merge(balance_diffs)?;

    Ok(ConflictReason::None)
}

async fn apply_transaction<B: StorageBackend>(
    storage: &B,
    message_id: &MessageId,
    transaction: &TransactionPayload,
    metadata: &mut WhiteFlagMetadata,
) -> Result<ConflictReason, Error> {
    match transaction.essence() {
        Essence::Regular(essence) => {
            apply_regular_essence(
                storage,
                message_id,
                &transaction.id(),
                essence,
                transaction.unlock_blocks(),
                metadata,
            )
            .await
        }
        essence => Err(Error::UnsupportedTransactionEssenceKind(essence.kind())),
    }
}

async fn apply_message<B: StorageBackend>(
    storage: &B,
    message_id: &MessageId,
    message: &Message,
    metadata: &mut WhiteFlagMetadata,
) -> Result<(), Error> {
    metadata.referenced_messages += 1;

    match message.payload() {
        Some(Payload::Transaction(transaction)) => {
            match apply_transaction(storage, message_id, transaction, metadata).await? {
                ConflictReason::None => metadata.included_messages.push(*message_id),
                conflict => metadata.excluded_conflicting_messages.push((*message_id, conflict)),
            }
        }
        _ => metadata.excluded_no_transaction_messages.push(*message_id),
    }

    Ok(())
}

async fn traversal<B: StorageBackend>(
    tangle: &MsTangle<B>,
    storage: &B,
    mut messages_ids: Vec<MessageId>,
    metadata: &mut WhiteFlagMetadata,
) -> Result<(), Error> {
    let mut visited = HashSet::new();

    while let Some(message_id) = messages_ids.last() {
        match tangle
            .get_vertex(&message_id)
            .await
            .as_ref()
            .and_then(|v| v.message_and_metadata().cloned())
        {
            Some((message, meta)) => {
                if meta.flags().is_referenced() {
                    visited.insert(*message_id);
                    messages_ids.pop();
                    continue;
                }

                let mut next = None;

                for parent in message.parents().iter() {
                    if !visited.contains(parent) {
                        next.replace(parent);
                        break;
                    }
                }

                match next {
                    Some(next) => messages_ids.push(*next),
                    None => {
                        apply_message(storage, message_id, &message, metadata).await?;
                        visited.insert(*message_id);
                        messages_ids.pop();
                    }
                }
            }
            None => {
                if !tangle.is_solid_entry_point(message_id).await {
                    return Err(Error::MissingMessage(*message_id));
                } else {
                    visited.insert(*message_id);
                    messages_ids.pop();
                    continue;
                }
            }
        }
    }

    Ok(())
}

pub async fn white_flag<B: StorageBackend>(
    tangle: &MsTangle<B>,
    storage: &B,
    messages_ids: Vec<MessageId>,
    metadata: &mut WhiteFlagMetadata,
) -> Result<(), Error> {
    traversal(tangle, storage, messages_ids.into_iter().rev().collect(), metadata).await?;

    metadata.merkle_proof = MerkleHasher::<Blake2b256>::new().digest(&metadata.included_messages);

    if metadata.referenced_messages
        != metadata.excluded_no_transaction_messages.len()
            + metadata.excluded_conflicting_messages.len()
            + metadata.included_messages.len()
    {
        return Err(Error::InvalidMessagesCount(
            metadata.referenced_messages,
            metadata.excluded_no_transaction_messages.len(),
            metadata.excluded_conflicting_messages.len(),
            metadata.included_messages.len(),
        ));
    }

    let diff_sum = metadata.balance_diffs.iter().map(|(_, diff)| diff.amount()).sum();

    if diff_sum != 0 {
        return Err(Error::NonZeroBalanceDiffSum(diff_sum));
    }

    Ok(())
}
