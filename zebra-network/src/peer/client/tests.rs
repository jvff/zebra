//! Tests for the [`Client`] part of peer connections, and some test utilities for mocking
//! [`Client`] instances.

mod vectors;

use std::time::Duration;

use futures::{
    channel::{mpsc, oneshot},
    future::{self, AbortHandle, Future, FutureExt},
};
use tokio::task::JoinHandle;

use crate::{
    peer::{error::SharedPeerError, CancelHeartbeatTask, Client, ClientRequest, ErrorSlot},
    protocol::external::types::Version,
};

/// The maximum time a mocked peer connection should be alive during a test.
const MAX_PEER_CONNECTION_TIME: Duration = Duration::from_secs(10);

/// A handle to a mocked [`Client`] instance.
pub struct MockedClientHandle {
    request_receiver: Option<mpsc::Receiver<ClientRequest>>,
    shutdown_receiver: Option<oneshot::Receiver<CancelHeartbeatTask>>,
    error_slot: ErrorSlot,
    version: Version,
    connection_aborter: AbortHandle,
    heartbeat_aborter: AbortHandle,
}

impl MockedClientHandle {
    /// Gets the peer protocol version associated to the [`Client`].
    pub fn version(&self) -> Version {
        self.version
    }

    /// Checks if the [`Client`] instance has not been dropped, which would have disconnected from
    /// the peer.
    pub fn is_connected(&mut self) -> bool {
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

    /// Stops the mock background task that handles incoming remote requests and replies.
    pub async fn stop_connection_task(&self) {
        self.connection_aborter.abort();

        // Allow the task to detect that it was aborted.
        tokio::task::yield_now().await;
    }

    /// Stops the mock background task that sends periodic heartbeats.
    pub async fn stop_heartbeat_task(&self) {
        self.heartbeat_aborter.abort();

        // Allow the task to detect that it was aborted.
        tokio::task::yield_now().await;
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
pub struct MockClientBuilder<C = future::Ready<()>, H = future::Ready<()>> {
    connection_task: Option<C>,
    heartbeat_task: Option<H>,
    version: Option<Version>,
}

impl MockClientBuilder<future::Ready<()>, future::Ready<()>> {
    /// Create a new default [`MockClientBuilder`].
    pub fn new() -> Self {
        MockClientBuilder {
            connection_task: None,
            heartbeat_task: None,
            version: None,
        }
    }
}

impl<C, H> MockClientBuilder<C, H>
where
    C: Future<Output = ()> + Send + 'static,
    H: Future<Output = ()> + Send + 'static,
{
    /// Configure the mocked peer's version.
    pub fn with_version(mut self, version: Version) -> Self {
        self.version = Some(version);
        self
    }

    /// Configure the mock connection task future to use.
    pub fn with_connection_task<NewC>(self, connection_task: NewC) -> MockClientBuilder<NewC, H> {
        MockClientBuilder {
            connection_task: Some(connection_task),
            heartbeat_task: self.heartbeat_task,
            version: self.version,
        }
    }

    /// Configure the mock heartbeat task future to use.
    pub fn with_heartbeat_task<NewH>(self, heartbeat_task: NewH) -> MockClientBuilder<C, NewH> {
        MockClientBuilder {
            connection_task: self.connection_task,
            heartbeat_task: Some(heartbeat_task),
            version: self.version,
        }
    }

    /// Build a [`Client`] instance with the mocked data and a [`MockedClientHandle`] to track it.
    pub fn build(self) -> (Client, MockedClientHandle) {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel();
        let (request_sender, request_receiver) = mpsc::channel(1);
        let error_slot = ErrorSlot::default();
        let version = self.version.unwrap_or(Version(0));

        let (connection_task, connection_aborter) =
            Self::spawn_background_task_or_fallback(self.connection_task);
        let (heartbeat_task, heartbeat_aborter) =
            Self::spawn_background_task_or_fallback(self.heartbeat_task);

        let client = Client {
            shutdown_tx: Some(shutdown_sender),
            server_tx: request_sender,
            error_slot: error_slot.clone(),
            version,
            connection_task,
            heartbeat_task,
        };

        let handle = MockedClientHandle {
            request_receiver: Some(request_receiver),
            shutdown_receiver: Some(shutdown_receiver),
            error_slot,
            version,
            connection_aborter,
            heartbeat_aborter,
        };

        (client, handle)
    }

    /// Spawn a mock background abortable task `task_future` if provided, or a fallback task
    /// otherwise.
    ///
    /// The fallback task lives as long as [`MAX_PEER_CONNECTION_TIME`].
    fn spawn_background_task_or_fallback<T>(task_future: Option<T>) -> (JoinHandle<()>, AbortHandle)
    where
        T: Future<Output = ()> + Send + 'static,
    {
        match task_future {
            Some(future) => Self::spawn_background_task(future),
            None => Self::spawn_background_task(tokio::time::sleep(MAX_PEER_CONNECTION_TIME)),
        }
    }

    /// Spawn a mock background abortable task to run `task_future`.
    fn spawn_background_task<T>(task_future: T) -> (JoinHandle<()>, AbortHandle)
    where
        T: Future<Output = ()> + Send + 'static,
    {
        let (task, abort_handle) = future::abortable(task_future);
        let task_handle = tokio::spawn(task.map(|_result| ()));

        (task_handle, abort_handle)
    }
}
