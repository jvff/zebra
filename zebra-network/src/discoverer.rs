use std::{
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use futures::Stream;
use pin_project::pin_project;
use tokio::sync::mpsc;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tower::{
    discover::{Change, Discover},
    load::{CompleteOnResponse, PeakEwma, PeakEwmaDiscover},
    BoxError,
};

use crate::{
    constants,
    peer::{Client, LoadTrackedClient},
};

type LoadTracker = PeakEwmaDiscover<UnboundedReceiverStream<DiscoveryEvent<Client>>>;
type DiscoveryEvent<C> = Result<Change<SocketAddr, C>, BoxError>;
type PollDiscover = Poll<Option<DiscoveryEvent<LoadTrackedClient>>>;

/// A collector of discovered peers that provides them as [`LoadTrackedClient`] services through a
/// [`Discover`] interface.
#[pin_project(project = PinnedPeerDiscoverer)]
pub struct PeerDiscoverer<D> {
    /// The incoming discovered peers, as a [`Stream`] of peer addresses and [`Client`] services.
    #[pin]
    discovered_peers: D,

    /// The internal tracker of peer loads.
    #[pin]
    load_tracker: LoadTracker,

    /// A channel to send received peer services to the load tracker.
    discovery_event_sender: Option<mpsc::UnboundedSender<DiscoveryEvent<Client>>>,
}

impl<D> PeerDiscoverer<D> {
    /// Create a new [`PeerDiscoverer`] to handle new peers reported in the `discovered_peers`
    /// stream.
    pub fn new(discovered_peers: D) -> Self {
        let (discovery_event_sender, discovery_event_receiver) = mpsc::unbounded_channel();

        let load_tracker = PeakEwmaDiscover::new(
            UnboundedReceiverStream::new(discovery_event_receiver),
            constants::EWMA_DEFAULT_RTT,
            constants::EWMA_DECAY_TIME,
            CompleteOnResponse::default(),
        );

        PeerDiscoverer {
            discovered_peers,
            load_tracker,
            discovery_event_sender: Some(discovery_event_sender),
        }
    }
}

/// [`PeerDiscover`] can be used as a [`Stream`] of discovery events.
///
/// This implementation allows [`PeerDiscover`] to be used through the [`Discover`] interface as
/// well.
impl<D> Stream for PeerDiscoverer<D>
where
    D: Stream<Item = (SocketAddr, Client)>,
{
    type Item = DiscoveryEvent<LoadTrackedClient>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> PollDiscover {
        let mut this = self.project();

        // Check if load tracker has finished preparing a peer service.
        match this.load_tracker.as_mut().poll_discover(context) {
            // No, so check if there are newly discovered peers to send to the load tracker.
            Poll::Pending => {
                this.forward_discovered_peers(context);

                Poll::Pending
            }

            // Yes, so finish preparing it and return it.
            Poll::Ready(Some(Ok(Change::Insert(address, load_tracked_service)))) => {
                let discover_event = this.finish_preparing_client(address, load_tracked_service);

                Poll::Ready(Some(discover_event))
            }

            // `Remove` is never sent because `PeerDiscoverer` never sends `Remove` to the load
            // tracker.
            Poll::Ready(Some(Ok(Change::Remove(_)))) => unreachable!("no peers are ever removed"),

            // An error occurred in the load tracker.
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error))),

            // The load tracker has stopped, as a consequence of `discovered_peers` having stopped
            // sending new peers.
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

impl<'p, D> PinnedPeerDiscoverer<'p, D>
where
    D: Stream<Item = (SocketAddr, Client)>,
{
    /// Try to forward newly discovered peers to the load tracker.
    ///
    /// Returns [`Poll::Pending`] because
    fn forward_discovered_peers(&mut self, context: &mut Context) {
        while let Poll::Ready(maybe_event) = self.discovered_peers.as_mut().poll_next(context) {
            match maybe_event {
                Some(incoming_event) => self.forward_discovered_peer(incoming_event),
                None => {
                    self.discovery_event_sender.take();
                }
            }
        }
    }

    /// Forward a newly discovered peer to the load tracker.
    fn forward_discovered_peer(&mut self, (address, client): (SocketAddr, Client)) {
        if let Some(event_sender) = self.discovery_event_sender.as_mut() {
            let event = Ok(Change::Insert(address, client));

            if event_sender.send(event).is_err() {
                self.discovery_event_sender.take();
            }
        }
    }

    /// Finish preparing a load tracked service into a discovery event.
    fn finish_preparing_client(
        &mut self,
        address: SocketAddr,
        load_tracked_service: PeakEwma<Client>,
    ) -> DiscoveryEvent<LoadTrackedClient> {
        let load_tracked_client = LoadTrackedClient::new(load_tracked_service);

        Ok(Change::Insert(address, load_tracked_client))
    }
}
