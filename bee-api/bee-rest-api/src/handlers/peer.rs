// Copyright 2020 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::{
    handlers::{BodyInner, SuccessBody},
    types::{GossipDto, PeerDto},
};

use bee_network::PeerId;
use bee_protocol::PeerManager;
use bee_runtime::resource::ResourceHandle;

use serde::{Deserialize, Serialize};
use warp::{reject, Rejection, Reply};

use crate::filters::CustomRejection::NotFound;
use crate::types::{HeartbeatDto, MetricsDto, RelationDto, peer_to_peer_dto};

pub(crate) async fn peer(peer_id: PeerId, peer_manager: ResourceHandle<PeerManager>) -> Result<impl Reply, Rejection> {
    match peer_manager.get(&peer_id).await {
        Some(peer_entry) => {
            let peer_dto = peer_to_peer_dto(&peer_entry.0, peer_manager);
            Ok(warp::reply::json(&SuccessBody::new(PeerResponse(peer_dto))))
        }
        None => Err(reject::custom(NotFound("peer not found".to_string()))),
    }
}

/// Response of GET /api/v1/peer/{peer_id}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PeerResponse(pub PeerDto);

impl BodyInner for PeerResponse {}