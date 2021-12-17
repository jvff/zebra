//! Fixed peer [`Client`] test vectors.

use zebra_test::service_extensions::IsReady;

use crate::{peer::MockClientBuilder, PeerError};

#[tokio::test]
async fn client_service_ready_ok() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    assert!(client.is_ready().await);
    assert!(handle.current_error().is_none());
    assert!(handle.wants_connection_heartbeats());
    assert!(handle.try_to_receive_request().is_empty());
}

#[tokio::test]
async fn client_service_ready_heartbeat_exit() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    handle.set_error(PeerError::HeartbeatTaskExited);
    handle.drop_heartbeat_shutdown_receiver();

    assert!(client.is_failed().await);
    assert!(handle.current_error().is_some());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_request_drop() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    handle.set_error(PeerError::ConnectionDropped);
    handle.drop_request_receiver();

    assert!(client.is_failed().await);
    assert!(handle.current_error().is_some());
    assert!(!handle.wants_connection_heartbeats());
}

#[tokio::test]
async fn client_service_ready_request_close() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    handle.set_error(PeerError::ConnectionClosed);
    handle.close_request_receiver();

    assert!(client.is_failed().await);
    assert!(handle.current_error().is_some());
    assert!(!handle.wants_connection_heartbeats());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_error_in_slot() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    handle.set_error(PeerError::Overloaded);

    assert!(client.is_failed().await);
    assert!(handle.current_error().is_some());
    assert!(!handle.wants_connection_heartbeats());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_multiple_errors() {
    zebra_test::init();

    let (mut client, mut handle) = MockClientBuilder::new().build();

    handle.set_error(PeerError::DuplicateHandshake);
    handle.drop_heartbeat_shutdown_receiver();
    handle.close_request_receiver();

    assert!(client.is_failed().await);
    assert!(handle.current_error().is_some());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_drop_cleanup() {
    zebra_test::init();

    let (client, mut handle) = MockClientBuilder::new().build();

    std::mem::drop(client);

    assert!(handle.current_error().is_some());
    assert!(!handle.wants_connection_heartbeats());
    assert!(handle.try_to_receive_request().is_closed());
}
