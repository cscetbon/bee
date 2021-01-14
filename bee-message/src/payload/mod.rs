// Copyright 2020 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

pub mod indexation;
pub mod milestone;
pub mod transaction;

use indexation::{Indexation, INDEXATION_PAYLOAD_TYPE};
use milestone::{Milestone, MILESTONE_PAYLOAD_TYPE};
use transaction::{Transaction, TRANSACTION_PAYLOAD_TYPE};

use crate::Error;

use bee_common::packable::{Packable, Read, Write};

use serde::{Deserialize, Serialize};

use alloc::boxed::Box;

#[non_exhaustive]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Payload {
    Transaction(Box<Transaction>),
    Milestone(Box<Milestone>),
    Indexation(Box<Indexation>),
}

impl From<Transaction> for Payload {
    fn from(payload: Transaction) -> Self {
        Self::Transaction(Box::new(payload))
    }
}

impl From<Milestone> for Payload {
    fn from(payload: Milestone) -> Self {
        Self::Milestone(Box::new(payload))
    }
}

impl From<Indexation> for Payload {
    fn from(payload: Indexation) -> Self {
        Self::Indexation(Box::new(payload))
    }
}

impl Packable for Payload {
    type Error = Error;

    fn packed_len(&self) -> usize {
        match self {
            Self::Transaction(payload) => TRANSACTION_PAYLOAD_TYPE.packed_len() + payload.packed_len(),
            Self::Milestone(payload) => MILESTONE_PAYLOAD_TYPE.packed_len() + payload.packed_len(),
            Self::Indexation(payload) => INDEXATION_PAYLOAD_TYPE.packed_len() + payload.packed_len(),
        }
    }

    fn pack<W: Write>(&self, writer: &mut W) -> Result<(), Self::Error> {
        match self {
            Self::Transaction(payload) => {
                TRANSACTION_PAYLOAD_TYPE.pack(writer)?;
                payload.pack(writer)?;
            }
            Self::Milestone(payload) => {
                MILESTONE_PAYLOAD_TYPE.pack(writer)?;
                payload.pack(writer)?;
            }
            Self::Indexation(payload) => {
                INDEXATION_PAYLOAD_TYPE.pack(writer)?;
                payload.pack(writer)?;
            }
        }

        Ok(())
    }

    fn unpack<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Self::Error>
    where
        Self: Sized,
    {
        Ok(match u32::unpack(reader)? {
            TRANSACTION_PAYLOAD_TYPE => Self::Transaction(Box::new(Transaction::unpack(reader)?)),
            MILESTONE_PAYLOAD_TYPE => Self::Milestone(Box::new(Milestone::unpack(reader)?)),
            INDEXATION_PAYLOAD_TYPE => Self::Indexation(Box::new(Indexation::unpack(reader)?)),
            _ => return Err(Self::Error::InvalidPayloadType),
        })
    }
}