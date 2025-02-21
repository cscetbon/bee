// Copyright 2020-2021 IOTA Stiftung
// SPDX-License-Identifier: Apache-2.0

use crate::workers::{
    error::Error,
    snapshot::{config::SnapshotConfig, error::Error as SnapshotError, import::import_snapshots},
    storage::{self, StorageBackend},
};

use bee_message::milestone::MilestoneIndex;
use bee_runtime::{node::Node, worker::Worker};
use bee_storage::{access::AsStream, backend::StorageBackend as _, health::StorageHealth};
use bee_tangle::{solid_entry_point::SolidEntryPoint, MsTangle, TangleWorker};

use async_trait::async_trait;

use chrono::{offset::TimeZone, Utc};
use futures::stream::StreamExt;
use log::info;

use std::any::TypeId;

pub struct SnapshotWorker {}

#[async_trait]
impl<N: Node> Worker<N> for SnapshotWorker
where
    N::Backend: StorageBackend,
{
    type Config = (u64, SnapshotConfig);
    type Error = Error;

    fn dependencies() -> &'static [TypeId] {
        vec![TypeId::of::<TangleWorker>()].leak()
    }

    async fn start(node: &mut N, config: Self::Config) -> Result<Self, Self::Error> {
        let (network_id, snapshot_config) = config;

        let tangle = node.resource::<MsTangle<N::Backend>>();
        let storage = node.storage();

        match storage::fetch_snapshot_info(&*storage).await? {
            None => {
                if let Err(e) = import_snapshots(&*storage, network_id, &snapshot_config).await {
                    (*storage)
                        .set_health(StorageHealth::Corrupted)
                        .await
                        .map_err(|e| Error::Storage(Box::new(e)))?;
                    return Err(e.into());
                }
            }
            Some(info) => {
                if info.network_id() != network_id {
                    return Err(Error::Snapshot(SnapshotError::NetworkIdMismatch(
                        info.network_id(),
                        network_id,
                    )));
                }

                info!(
                    "Loaded snapshot from {} with snapshot index {}, entry point index {} and pruning index {}.",
                    Utc.timestamp(info.timestamp() as i64, 0).format("%d-%m-%Y %H:%M:%S"),
                    *info.snapshot_index(),
                    *info.entry_point_index(),
                    *info.pruning_index(),
                );
            }
        }

        let mut solid_entry_points = AsStream::<SolidEntryPoint, MilestoneIndex>::stream(&*storage)
            .await
            .map_err(|e| Error::Storage(Box::new(e)))?;

        while let Some((sep, index)) = solid_entry_points.next().await {
            tangle.add_solid_entry_point(sep, index).await;
        }

        // Unwrap is fine because we just inserted the ledger index.
        // TODO unwrap
        let ledger_index = storage::fetch_ledger_index(&*storage).await.unwrap().unwrap();
        let snapshot_info = storage::fetch_snapshot_info(&*storage).await?.unwrap();

        tangle.update_snapshot_index(snapshot_info.snapshot_index());
        tangle.update_pruning_index(snapshot_info.pruning_index());
        tangle.update_solid_milestone_index(MilestoneIndex(*ledger_index));
        tangle.update_confirmed_milestone_index(MilestoneIndex(*ledger_index));
        tangle.update_latest_milestone_index(MilestoneIndex(*ledger_index));

        Ok(Self {})
    }
}
