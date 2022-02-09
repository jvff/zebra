//! `start` subcommand - entry point for starting a zebra node
//!
//! ## Application Structure
//!
//! A zebra node consists of the following services and tasks:
//!
//! Peers:
//!  * Peer Connection Pool Service
//!    * primary external interface for outbound requests from this node to remote peers
//!    * accepts requests from services and tasks in this node, and sends them to remote peers
//!  * Peer Discovery Service
//!    * maintains a list of peer addresses, and connection priority metadata
//!    * discovers new peer addresses from existing peer connections
//!    * initiates new outbound peer connections in response to demand from tasks within this node
//!
//! Blocks & Mempool Transactions:
//!  * Consensus Service
//!    * handles all validation logic for the node
//!    * verifies blocks using zebra-chain, then stores verified blocks in zebra-state
//!    * verifies mempool and block transactions using zebra-chain and zebra-script,
//!      and returns verified mempool transactions for mempool storage
//!  * Groth16 Parameters Download Task
//!    * downloads the Sprout and Sapling Groth16 circuit parameter files
//!    * finishes when the download is complete and the download file hashes have been checked
//!  * Inbound Service
//!    * primary external interface for inbound peer requests to this node
//!    * handles requests from peers for network data, chain data, and mempool transactions
//!    * spawns download and verify tasks for each gossiped block
//!    * sends gossiped transactions to the mempool service
//!
//! Blocks:
//!  * Sync Task
//!    * runs in the background and continuously queries the network for
//!      new blocks to be verified and added to the local state
//!    * spawns download and verify tasks for each crawled block
//!  * State Service
//!    * contextually verifies blocks
//!    * handles in-memory storage of multiple non-finalized chains
//!    * handles permanent storage of the best finalized chain
//!  * Block Gossip Task
//!    * runs in the background and continuously queries the state for
//!      newly committed blocks to be gossiped to peers
//!  * Progress Task
//!    * logs progress towards the chain tip
//!
//! Mempool Transactions:
//!  * Mempool Service
//!    * activates when the syncer is near the chain tip
//!    * spawns download and verify tasks for each crawled or gossiped transaction
//!    * handles in-memory storage of unmined transactions
//!  * Queue Checker Task
//!    * runs in the background, polling the mempool to store newly verified transactions
//!  * Transaction Gossip Task
//!    * runs in the background and gossips newly added mempool transactions
//!      to peers

use std::{cmp::max, time::Duration};

use abscissa_core::{config, Command, FrameworkError, Options, Runnable};
use chrono::Utc;
use color_eyre::eyre::{eyre, Report};
use futures::FutureExt;
use tokio::{pin, select, sync::oneshot};
use tower::{builder::ServiceBuilder, util::BoxService};
use tracing_futures::Instrument;

use zebra_chain::{chain_tip::ChainTip, parameters::Network};

use crate::{
    components::{
        inbound::{self, InboundSetupData},
        mempool::{self, Mempool},
        sync::{self, SyncStatus},
        tokio::{RuntimeRun, TokioComponent},
        ChainSync, Inbound,
    },
    config::ZebradConfig,
    prelude::*,
};

/// `start` subcommand
#[derive(Command, Debug, Options)]
pub struct StartCmd {
    /// Filter strings which override the config file and defaults
    #[options(free, help = "tracing filters which override the zebrad.toml config")]
    filters: Vec<String>,
}

impl StartCmd {
    async fn start(&self) -> Result<(), Report> {
        let config = app_config().clone();
        info!(?config);

        info!("initializing node state");
        let (state_service, latest_chain_tip, chain_tip_change) =
            zebra_state::init(config.state.clone(), config.network.network);
        let state = ServiceBuilder::new()
            .buffer(Self::state_buffer_bound())
            .service(state_service);

        info!("initializing network");
        // The service that our node uses to respond to requests by peers. The
        // load_shed middleware ensures that we reduce the size of the peer set
        // in response to excess load.
        let (setup_tx, setup_rx) = oneshot::channel();
        let inbound = ServiceBuilder::new()
            .load_shed()
            .buffer(inbound::downloads::MAX_INBOUND_CONCURRENCY)
            .service(Inbound::new(setup_rx));

        let (peer_set, address_book) =
            zebra_network::init(config.network.clone(), inbound, latest_chain_tip.clone()).await;

        info!("initializing verifiers");
        let (chain_verifier, tx_verifier, mut groth16_download_handle) =
            zebra_consensus::chain::init(
                config.consensus.clone(),
                config.network.network,
                state.clone(),
                config.consensus.debug_skip_parameter_preload,
            )
            .await;

        info!("initializing syncer");
        let (syncer, sync_status) = ChainSync::new(
            &config,
            peer_set.clone(),
            chain_verifier.clone(),
            state.clone(),
            latest_chain_tip.clone(),
        );

        info!("initializing mempool");
        let (mempool, mempool_transaction_receiver) = Mempool::new(
            &config.mempool,
            peer_set.clone(),
            state.clone(),
            tx_verifier,
            sync_status.clone(),
            latest_chain_tip.clone(),
            chain_tip_change.clone(),
        );
        let mempool = BoxService::new(mempool);
        let mempool = ServiceBuilder::new()
            .buffer(mempool::downloads::MAX_INBOUND_CONCURRENCY)
            .service(mempool);

        let setup_data = InboundSetupData {
            address_book,
            block_download_peer_set: peer_set.clone(),
            block_verifier: chain_verifier,
            mempool: mempool.clone(),
            state,
            latest_chain_tip: latest_chain_tip.clone(),
        };
        setup_tx
            .send(setup_data)
            .map_err(|_| eyre!("could not send setup data to inbound service"))?;

        let syncer_task_handle = tokio::spawn(syncer.sync().in_current_span());

        let mut block_gossip_task_handle = tokio::spawn(
            sync::gossip_best_tip_block_hashes(
                sync_status.clone(),
                chain_tip_change.clone(),
                peer_set.clone(),
            )
            .in_current_span(),
        );

        let mempool_crawler_task_handle = mempool::Crawler::spawn(
            &config.mempool,
            peer_set.clone(),
            mempool.clone(),
            sync_status.clone(),
            chain_tip_change,
        );

        let mempool_queue_checker_task_handle = mempool::QueueChecker::spawn(mempool);

        let tx_gossip_task_handle = tokio::spawn(
            mempool::gossip_mempool_transaction_id(mempool_transaction_receiver, peer_set)
                .in_current_span(),
        );

        let progress_task_handle = tokio::spawn(
            Self::update_progress(config.network.network, latest_chain_tip, sync_status)
                .in_current_span(),
        );

        info!("spawned initial Zebra tasks");

        // TODO: put tasks into an ongoing FuturesUnordered and a startup FuturesUnordered?

        // ongoing tasks
        pin!(syncer_task_handle);
        pin!(mempool_crawler_task_handle);
        pin!(mempool_queue_checker_task_handle);
        pin!(tx_gossip_task_handle);
        pin!(progress_task_handle);

        // startup tasks
        let groth16_download_handle_fused = (&mut groth16_download_handle).fuse();
        pin!(groth16_download_handle_fused);

        // Wait for tasks to finish
        let exit_status = loop {
            let mut exit_when_task_finishes = true;

            let result = select! {
                sync_result = &mut syncer_task_handle => sync_result
                    .expect("unexpected panic in the syncer task")
                    .map(|_| info!("syncer task exited")),

                block_gossip_result = &mut block_gossip_task_handle => block_gossip_result
                    .expect("unexpected panic in the chain tip block gossip task")
                    .map(|_| info!("chain tip block gossip task exited"))
                    .map_err(|e| eyre!(e)),

                mempool_crawl_result = &mut mempool_crawler_task_handle => mempool_crawl_result
                    .expect("unexpected panic in the mempool crawler")
                    .map(|_| info!("mempool crawler task exited"))
                    .map_err(|e| eyre!(e)),

                mempool_queue_result = &mut mempool_queue_checker_task_handle => mempool_queue_result
                    .expect("unexpected panic in the mempool queue checker")
                    .map(|_| info!("mempool queue checker task exited"))
                    .map_err(|e| eyre!(e)),

                tx_gossip_result = &mut tx_gossip_task_handle => tx_gossip_result
                    .expect("unexpected panic in the transaction gossip task")
                    .map(|_| info!("transaction gossip task exited"))
                    .map_err(|e| eyre!(e)),

                progress_result = &mut progress_task_handle => {
                    progress_result
                        .expect("unexpected panic in the chain progress task");
                    info!("chain progress task exited");
                    Ok(())
                }

                // Unlike other tasks, we expect the download task to finish while Zebra is running.
                groth16_download_result = &mut groth16_download_handle_fused => {
                    groth16_download_result
                        .unwrap_or_else(|_| panic!(
                            "unexpected panic in the Groth16 pre-download and check task. {}",
                            zebra_consensus::groth16::Groth16Parameters::failure_hint())
                        );

                    exit_when_task_finishes = false;
                    Ok(())
                }
            };

            // Stop Zebra if a task finished and returned an error,
            // or if an ongoing task exited.
            if let Err(err) = result {
                break Err(err);
            }

            if exit_when_task_finishes {
                break Ok(());
            }
        };

        info!("exiting Zebra because an ongoing task exited: stopping other tasks");

        // ongoing tasks
        syncer_task_handle.abort();
        block_gossip_task_handle.abort();
        mempool_crawler_task_handle.abort();
        mempool_queue_checker_task_handle.abort();
        tx_gossip_task_handle.abort();

        // startup tasks
        groth16_download_handle.abort();

        exit_status
    }

    /// Returns the bound for the state service buffer,
    /// based on the configurations of the services that use the state concurrently.
    fn state_buffer_bound() -> usize {
        let config = app_config().clone();

        // TODO: do we also need to account for concurrent use across services?
        //       we could multiply the maximum by 3/2, or add a fixed constant
        max(
            config.sync.max_concurrent_block_requests,
            max(
                inbound::downloads::MAX_INBOUND_CONCURRENCY,
                mempool::downloads::MAX_INBOUND_CONCURRENCY,
            ),
        )
    }

    /// Logs Zebra's estimated progress towards the chain tip.
    async fn update_progress(
        network: Network,
        latest_chain_tip: impl ChainTip,
        sync_status: SyncStatus,
    ) {
        // The amount of time between progress logs.
        const LOG_INTERVAL: Duration = Duration::from_secs(60);

        // The number of blocks we consider to be close to the tip.
        //
        // Most chain forks are 1-7 blocks long.
        const MAX_CLOSE_TO_TIP_BLOCKS: i32 = 1;

        // Skip slow sync warnings when we are this close to the tip.
        //
        // In testing, we've seen warnings around 30 blocks.
        //
        // TODO: replace with `MAX_CLOSE_TO_TIP_BLOCKS` after fixing slow syncing near tip (#3375)
        const MIN_SYNC_WARNING_BLOCKS: i32 = 60;

        loop {
            let _now = Utc::now();
            let is_close_to_tip = sync_status.is_close_to_tip();

            if let Some(estimated_height) =
                latest_chain_tip.estimate_network_chain_tip_height(network /*, now*/)
            {
                // The estimate/actual race doesn't matter here,
                // because we're only using it for metrics and logging.
                let current_height = latest_chain_tip
                    .best_tip_height()
                    .expect("unexpected empty state: estimate requires a block height");

                let sync_progress = f64::from(current_height.0) / f64::from(estimated_height.0);
                let sync_percent = format!("{:.3}", sync_progress * 100.0);

                let remaining_sync_blocks = estimated_height - current_height;

                // TODO:
                // - estimate the remaining sync time
                // - log progress, remaining blocks, and remaining time to next network upgrade
                // - also add this info to the metrics

                if is_close_to_tip && remaining_sync_blocks > MIN_SYNC_WARNING_BLOCKS {
                    // We've stopped syncing blocks, but we estimate we're a long way from the tip.
                    //
                    // TODO: warn after fixing slow syncing near tip (#3375)
                    info!(
                        %sync_percent,
                        ?remaining_sync_blocks,
                        "initial sync might have stalled, or estimated tip is wrong. \
                         Hint: check that your computer's clock and time zone are correct"
                    );
                } else if is_close_to_tip {
                    // We've stayed near the tip for a while, and we've stopped syncing lots of blocks.
                    //
                    // TODO: reduce this log level after full sync tests are merged,
                    //       and have been stable for a while (#1592)
                    info!(
                        %sync_percent,
                        ?remaining_sync_blocks,
                        "finished initial sync to chain tip"
                    );
                } else if remaining_sync_blocks <= MAX_CLOSE_TO_TIP_BLOCKS {
                    // We estimate we're near the tip, but we have been syncing lots of blocks recently.
                    info!(
                        %sync_percent,
                        ?remaining_sync_blocks,
                        "finished initial sync to chain tip"
                    );
                } else {
                    // We estimate we're far from the tip, and we've been syncing lots of blocks.
                    info!(
                        %sync_percent,
                        ?remaining_sync_blocks,
                        "estimated progress to chain tip"
                    );
                }
            }

            tokio::time::sleep(LOG_INTERVAL).await;
        }
    }
}

impl Runnable for StartCmd {
    /// Start the application.
    fn run(&self) {
        info!("Starting zebrad");
        let rt = app_writer()
            .state_mut()
            .components
            .get_downcast_mut::<TokioComponent>()
            .expect("TokioComponent should be available")
            .rt
            .take();

        rt.expect("runtime should not already be taken")
            .run(self.start());

        info!("stopping zebrad");
    }
}

impl config::Override<ZebradConfig> for StartCmd {
    // Process the given command line options, overriding settings from
    // a configuration file using explicit flags taken from command-line
    // arguments.
    fn override_config(&self, mut config: ZebradConfig) -> Result<ZebradConfig, FrameworkError> {
        if !self.filters.is_empty() {
            config.tracing.filter = Some(self.filters.join(","));
        }

        Ok(config)
    }
}
