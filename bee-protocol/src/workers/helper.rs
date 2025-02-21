// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{
    types::metrics::NodeMetrics,
    workers::{
        packets::Heartbeat, peer::PeerManager, sender::Sender, storage::StorageBackend, MessageRequesterWorker,
        MessageRequesterWorkerEvent, MilestoneRequesterWorkerEvent, RequestedMessages, RequestedMilestones,
    },
};

use bee_message::{milestone::MilestoneIndex, MessageId};
use bee_network::PeerId;
use bee_tangle::MsTangle;

use log::warn;
use tokio::sync::mpsc;

// MilestoneRequest

pub(crate) async fn request_milestone<B: StorageBackend>(
    tangle: &MsTangle<B>,
    milestone_requester: &mpsc::UnboundedSender<MilestoneRequesterWorkerEvent>,
    requested_milestones: &RequestedMilestones,
    index: MilestoneIndex,
    to: Option<PeerId>,
) {
    if !requested_milestones.contains(&index).await && !tangle.contains_milestone(index).await {
        if let Err(e) = milestone_requester.send(MilestoneRequesterWorkerEvent(index, to)) {
            warn!("Requesting milestone failed: {}.", e);
        }
    }
}

pub(crate) async fn request_latest_milestone<B: StorageBackend>(
    tangle: &MsTangle<B>,
    milestone_requester: &mpsc::UnboundedSender<MilestoneRequesterWorkerEvent>,
    requested_milestones: &RequestedMilestones,
    to: Option<PeerId>,
) {
    request_milestone(tangle, milestone_requester, requested_milestones, MilestoneIndex(0), to).await
}

// MessageRequest

pub async fn request_message<B: StorageBackend>(
    tangle: &MsTangle<B>,
    message_requester: &MessageRequesterWorker,
    requested_messages: &RequestedMessages,
    message_id: MessageId,
    index: MilestoneIndex,
) {
    if !tangle.contains(&message_id).await
        && !tangle.is_solid_entry_point(&message_id).await
        && !requested_messages.contains(&message_id).await
    {
        message_requester.request(MessageRequesterWorkerEvent(message_id, index));
    }
}

// Heartbeat

pub async fn send_heartbeat<B: StorageBackend>(
    peer_manager: &PeerManager,
    metrics: &NodeMetrics,
    tangle: &MsTangle<B>,
    to: &PeerId,
) {
    Sender::<Heartbeat>::send(
        peer_manager,
        metrics,
        to,
        Heartbeat::new(
            *tangle.get_solid_milestone_index(),
            *tangle.get_pruning_index(),
            *tangle.get_latest_milestone_index(),
            peer_manager.connected_peers().await,
            peer_manager.synced_peers().await,
        ),
    )
    .await;
}

pub async fn broadcast_heartbeat<B: StorageBackend>(
    peer_manager: &PeerManager,
    metrics: &NodeMetrics,
    tangle: &MsTangle<B>,
) {
    // TODO bring it back
    //    peer_manager.for_each_peer(|peer_id, _| async {
    for (peer_id, _) in peer_manager.peers.read().await.iter() {
        send_heartbeat(peer_manager, metrics, tangle, &peer_id).await
    }
}
