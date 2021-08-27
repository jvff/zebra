//! Zebra Mempool crawler.
//!
//! The crawler periodically requests transactions from peers in order to populate the mempool.

use std::time::Duration;

use futures::{
    stream::{self, FuturesUnordered},
    StreamExt, TryFutureExt, TryStreamExt,
};
use tokio::{sync::Mutex, task::JoinHandle, time::sleep};
use tower::{timeout::Timeout, BoxError, Service, ServiceExt};

use zebra_network::{Request, Response};

#[cfg(test)]
mod tests;

/// The number of peers to request transactions from per crawl event.
const FANOUT: usize = 4;

/// The delay between crawl events.
const RATE_LIMIT_DELAY: Duration = Duration::from_secs(75);

/// The time to wait for a peer response.
///
/// # Correctness
///
/// If this timeout is removed or set too high, the crawler may hang waiting for a peer to respond.
///
/// If this timeout is set too low, the crawler may fail to populate the mempool.
const PEER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(6);

/// The mempool transaction crawler.
pub struct Crawler<S> {
    peer_set: Mutex<Timeout<S>>,
}

impl<S> Crawler<S>
where
    S: Service<Request, Response = Response, Error = BoxError> + Clone + Send + 'static,
    S::Future: Send,
{
    /// Spawn an asynchronous task to run the mempool crawler.
    pub fn spawn(peer_set: S) -> JoinHandle<Result<(), BoxError>> {
        let crawler = Crawler {
            peer_set: Mutex::new(Timeout::new(peer_set, PEER_RESPONSE_TIMEOUT)),
        };

        tokio::spawn(crawler.run())
    }

    /// Periodically crawl peers for transactions to include in the mempool.
    pub async fn run(self) -> Result<(), BoxError> {
        loop {
            self.wait_until_enabled().await;
            self.crawl_transactions().await?;
            sleep(RATE_LIMIT_DELAY).await;
        }
    }

    /// Wait until the mempool is enabled.
    async fn wait_until_enabled(&self) {
        // TODO: Check if synchronizing up to chain tip has finished (#2603).
    }

    /// Crawl peers for transactions.
    ///
    /// Concurrently request [`FANOUT`] peers for transactions to include in the mempool.
    async fn crawl_transactions(&self) -> Result<(), BoxError> {
        let peer_set = self.peer_set.lock().await.clone();

        trace!("Crawling for mempool transactions");

        // get readiness for one peer at a time, to avoid peer set contention
        let requests = stream::repeat_with(move || peer_set.clone())
            .take(FANOUT)
            .then(|peer_set| {
                peer_set.ready_oneshot().map_ok(|mut ready_peer_set| {
                    ready_peer_set.call(Request::MempoolTransactionIds)
                })
            })
            // end the task on permanent peer set errors
            .try_collect::<FuturesUnordered<_>>()
            .await?;

        requests
            .and_then(|response| self.handle_response(response))
            // TODO: Reduce the log level of the errors (#2655).
            .inspect_err(|error| info!("Failed to crawl peer for mempool transactions: {}", error))
            .for_each(|_| async {})
            .await;

        Ok(())
    }

    /// Handle a peer's response to the crawler's request for transactions.
    async fn handle_response(&self, response: Response) -> Result<(), BoxError> {
        let transaction_ids = match response {
            Response::TransactionIds(ids) => ids,
            _ => unreachable!("Peer set did not respond with transaction IDs to mempool crawler"),
        };

        trace!(
            "Mempool crawler received {} transaction IDs",
            transaction_ids.len()
        );

        // TODO: Send transaction IDs to the download and verify stream (#2650)

        Ok(())
    }
}
