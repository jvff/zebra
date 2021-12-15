//! Tests for the [`Client`] part of peer connections, and some test utilities for mocking
//! [`Client`] instances.

mod vectors;

use futures::channel::{mpsc, oneshot};

use crate::{
    peer::{error::SharedPeerError, CancelHeartbeatTask, Client, ClientRequest, ErrorSlot},
    protocol::external::types::Version,
};

/// A handle to a mocked [`Client`] instance.
pub struct MockedClientHandle {
    request_receiver: mpsc::Receiver<ClientRequest>,
    shutdown_receiver: oneshot::Receiver<CancelHeartbeatTask>,
    error_slot: ErrorSlot,
    version: Version,
}

impl MockedClientHandle {
    /// Create a new mocked [`Client`] instance, returning it together with a handle to track it.
    pub fn new(version: Version) -> (Self, Client) {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (request_sender, request_receiver) = mpsc::channel(1);
        let error_slot = ErrorSlot::default();

        let client = Client {
            shutdown_tx: Some(shutdown_sender),
            server_tx: request_sender,
            error_slot: error_slot.clone(),
            version,
        };

        let handle = MockedClientHandle {
            request_receiver,
            shutdown_receiver,
            error_slot,
            version,
        };

        (handle, client)
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
            Ok(Some(CancelHeartbeatTask)) | Err(oneshot::Canceled) => false,
        }
    }

    /// Tries to receive a [`ClientRequest`] sent by the mocked [`Client`] instance.
    pub(crate) fn try_to_receive_request(&mut self) -> ReceiveRequestAttempt {
        match self.request_receiver.try_next() {
            Ok(Some(request)) => ReceiveRequestAttempt::Request(request),
            Ok(None) => ReceiveRequestAttempt::Closed,
            Err(_) => ReceiveRequestAttempt::Empty,
        }
    }

    /// Returns the current error in the [`ErrorSlot`], if there is one.
    pub fn current_error(&self) -> Option<SharedPeerError> {
        self.error_slot.try_get_error()
    }
}

/// A representation of the result of an attempt to receive a [`ClientRequest`] sent by the mocked
/// [`Client`] instance.
pub(crate) enum ReceiveRequestAttempt {
    /// The mocked [`Client`] instance has closed the sender endpoint of the channel.
    Closed,

    /// There were no queued requests in the channel.
    Empty,

    /// One request was successfully received.
    Request(ClientRequest),
}

impl ReceiveRequestAttempt {
    /// Check if the attempt to receive resulted in discovering that the sender endpoint had been
    /// closed.
    pub fn is_closed(&self) -> bool {
        matches!(self, ReceiveRequestAttempt::Closed)
    }

    /// Check if the attempt to receive resulted in no requests.
    pub fn is_empty(&self) -> bool {
        matches!(self, ReceiveRequestAttempt::Empty)
    }

    /// Returns the received request, if there was one.
    #[allow(dead_code)]
    pub fn request(self) -> Option<ClientRequest> {
        match self {
            ReceiveRequestAttempt::Request(request) => Some(request),
            ReceiveRequestAttempt::Closed | ReceiveRequestAttempt::Empty => None,
        }
    }
}
