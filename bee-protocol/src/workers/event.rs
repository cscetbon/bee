// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use bee_message::MessageId;

#[derive(Clone)]
pub struct MessageProcessed {
    pub message_id: MessageId,
}

#[derive(Clone)]
pub struct MessageSolidified {
    pub message_id: MessageId,
}

#[derive(Clone)]
pub struct MpsMetricsUpdated {
    pub incoming: u64,
    pub new: u64,
    pub known: u64,
    pub invalid: u64,
    pub outgoing: u64,
}

#[derive(Clone)]
pub struct NewVertex {
    pub id: String,
    pub parent_ids: Vec<String>,
    pub is_solid: bool,
    pub is_referenced: bool,
    pub is_conflicting: bool,
    pub is_milestone: bool,
    pub is_tip: bool,
    pub is_selected: bool,
}

#[derive(Clone)]
pub struct TipAdded {
    pub tip: MessageId,
}

#[derive(Clone)]
pub struct TipRemoved {
    pub tip: MessageId,
}
