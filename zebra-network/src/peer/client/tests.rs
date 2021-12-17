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
    request_receiver: Option<mpsc::Receiver<ClientRequest>>,
    shutdown_receiver: Option<oneshot::Receiver<CancelHeartbeatTask>>,
    error_slot: ErrorSlot,
    version: Version,
}

impl MockedClientHandle {
    /// Gets the peer protocol version associated to the [`Client`].
    pub fn version(&self) -> Version {
        self.version
    }

    /// Returns true if the [`Client`] instance still wants connection heartbeats to be sent.
    ///
    /// Checks that the client:
    /// - has not been dropped,
    /// - has not closed or dropped the mocked heartbeat task channel, and
    /// - has not asked the mocked heartbeat task to shut down.
    pub fn wants_connection_heartbeats(&mut self) -> bool {
        let receive_result = self
            .shutdown_receiver
            .as_mut()
            .expect("shutdown receiver has been dropped")
            .try_recv();

        match receive_result {
            Ok(None) => true,
            Ok(Some(CancelHeartbeatTask)) | Err(oneshot::Canceled) => false,
        }
    }

    /// Drops the shutdown receiver endpoint.
    pub fn drop_shutdown_receiver(&mut self) {
        let _ = self
            .shutdown_receiver
            .take()
            .expect("request receiver endpoint has already been dropped");
    }

    /// Closes the request receiver endpoint.
    pub fn close_request_receiver(&mut self) {
        self.request_receiver
            .as_mut()
            .expect("request receiver endpoint has been dropped")
            .close();
    }

    /// Drops the request receiver endpoint, forcefully closing the channel.
    pub fn drop_request_receiver(&mut self) {
        self.request_receiver
            .take()
            .expect("request receiver endpoint has already been dropped");
    }

    /// Tries to receive a [`ClientRequest`] sent by the mocked [`Client`] instance.
    pub(crate) fn try_to_receive_request(&mut self) -> ReceiveRequestAttempt {
        let receive_result = self
            .request_receiver
            .as_mut()
            .expect("request receiver endpoint has been dropped")
            .try_next();

        match receive_result {
            Ok(Some(request)) => ReceiveRequestAttempt::Request(request),
            Ok(None) => ReceiveRequestAttempt::Closed,
            Err(_) => ReceiveRequestAttempt::Empty,
        }
    }

    /// Returns the current error in the [`ErrorSlot`], if there is one.
    pub fn current_error(&self) -> Option<SharedPeerError> {
        self.error_slot.try_get_error()
    }

    /// Sets the error in the [`ErrorSlot`], assuming there isn't one already.
    ///
    /// # Panics
    ///
    /// If there's already an error in the [`ErrorSlot`].
    pub fn set_error(&self, error: impl Into<SharedPeerError>) {
        self.error_slot
            .try_update_error(error.into())
            .expect("unexpected earlier error in error slot")
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

/// A builder for a [`Client`] and [`MockedClientHandle`] instance.
///
/// Mocked data is used to construct a real [`Client`] instance. The mocked data is initialized by
/// the [`MockClientBuilder`], and can be accessed and changed through the [`MockedClientHandle`].
#[derive(Default)]
pub struct MockClientBuilder {
    version: Option<Version>,
}

impl MockClientBuilder {
    /// Create a new default [`MockClientBuilder`].
    pub fn new() -> Self {
        MockClientBuilder::default()
    }

    /// Configure the mocked peer's version.
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Build a [`Client`] instance with the mocked data and a [`MockedClientHandle`] to track it.
    pub fn build(self) -> (Client, MockedClientHandle) {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (request_sender, request_receiver) = mpsc::channel(1);
        let error_slot = ErrorSlot::default();
        let version = self.version.unwrap_or(Version(0));

        let client = Client {
            shutdown_tx: Some(shutdown_sender),
            server_tx: request_sender,
            error_slot: error_slot.clone(),
            version,
        };

        let handle = MockedClientHandle {
            request_receiver: Some(request_receiver),
            shutdown_receiver: Some(shutdown_receiver),
            error_slot,
            version,
        };

        (client, handle)
    }
}
