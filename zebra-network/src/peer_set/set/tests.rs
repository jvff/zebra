use futures::channel::{mpsc, oneshot};
use proptest::{collection::vec, prelude::any};
use proptest_derive::Arbitrary;

use crate::{
    peer::{Client, ClientRequest, ErrorSlot, LoadTrackedClient},
    protocol::external::types::Version,
};

/// The maximum number of arbitrary peers to generate in [`PeerVersions`].
///
/// This affects the maximum number of peer connections added to the [`PeerSet`] during the tests.
const MAX_PEERS: usize = 20;

/// A handle to a mocked [`Client`] instance.
struct MockedClientHandle {
    request_receiver: mpsc::Receiver<ClientRequest>,
    shutdown_receiver: oneshot::Receiver<()>,
    version: Version,
}

impl MockedClientHandle {
    /// Create a new mocked [`Client`] instance, returning it together with a handle to track it.
    pub fn new(version: Version) -> (Self, LoadTrackedClient) {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (request_sender, request_receiver) = mpsc::channel(1);

        let client = Client {
            shutdown_tx: Some(shutdown_sender),
            server_tx: request_sender,
            error_slot: ErrorSlot::default(),
            version,
        };

        let handle = MockedClientHandle {
            request_receiver,
            shutdown_receiver,
            version,
        };

        (handle, client.into())
    }

    /// Gets the peer protocol version associated to the [`Client`].
    pub fn version(&self) -> Version {
        self.version
    }

    /// Checks if the [`Client`] instance has not been dropped, which would have disconnected from
    /// the peer.
    pub fn is_connected(&mut self) -> bool {
        match self.shutdown_receiver.try_recv() {
            Ok(None) => true,
            Ok(Some(())) | Err(oneshot::Canceled) => false,
        }
    }
}

/// A helper type to generate arbitrary peer versions which can then become mock peer services.
#[derive(Arbitrary, Debug)]
struct PeerVersions {
    #[proptest(strategy = "vec(any::<Version>(), 1..MAX_PEERS)")]
    peer_versions: Vec<Version>,
}

impl PeerVersions {
    /// Convert the arbitrary peer versions into mock peer services.
    ///
    /// Each peer versions results in a mock peer service, which is returned as a tuple. The first
    /// element is the [`LeadTrackedClient`], which is the actual service for the peer connection.
    /// The second element is a [`MockedClientHandle`], which contains the open endpoints of the
    /// mock channels used by the peer service.
    pub fn mock_peers(&self) -> (Vec<LoadTrackedClient>, Vec<MockedClientHandle>) {
        let mut clients = Vec::with_capacity(self.peer_versions.len());
        let mut handles = Vec::with_capacity(self.peer_versions.len());

        for peer_version in &self.peer_versions {
            let (handle, client) = MockedClientHandle::new(*peer_version);

            clients.push(client);
            handles.push(handle);
        }

        (clients, handles)
    }
}
