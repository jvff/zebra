//! Fixed peer [`Client`] test vectors.

use futures::poll;
use tower::ServiceExt;

use zebra_test::service_extensions::IsReady;

use crate::{peer::ClientTestHarness, PeerError};

#[tokio::test]
async fn client_service_ready_ok() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    assert!(client.is_ready().await);
    assert!(harness.current_error().is_none());
    assert!(harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_empty());
}

#[tokio::test]
async fn client_service_ready_heartbeat_exit() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.set_error(PeerError::HeartbeatTaskExited);
    harness.drop_heartbeat_shutdown_receiver();

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_request_drop() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.set_error(PeerError::ConnectionDropped);
    harness.drop_outbound_client_request_receiver();

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
}

#[tokio::test]
async fn client_service_ready_request_close() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.set_error(PeerError::ConnectionClosed);
    harness.close_outbound_client_request_receiver();

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_error_in_slot() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.set_error(PeerError::Overloaded);

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_multiple_errors() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.set_error(PeerError::DuplicateHandshake);
    harness.drop_heartbeat_shutdown_receiver();
    harness.close_outbound_client_request_receiver();

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

#[tokio::test]
async fn client_service_drop_cleanup() {
    zebra_test::init();

    let (client, mut harness) = ClientTestHarness::build().finish();

    std::mem::drop(client);

    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

/// Force the connection background task to stop, and check if the `Client` properly handles it.
#[tokio::test]
async fn client_service_handles_exited_connection_task() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.stop_connection_task().await;

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

/// Force the connection background task to panic, and check if the `Client` propagates it.
#[tokio::test]
#[should_panic]
async fn client_service_propagates_panic_from_connection_task() {
    zebra_test::init();

    let (mut client, _harness) = ClientTestHarness::build()
        .with_connection_task(async move {
            panic!("connection task failure");
        })
        .finish();

    // Allow the custom connection task to run.
    tokio::task::yield_now().await;

    let _ = poll!(client.ready());
}

/// Force the heartbeat background task to stop, and check if the `Client` properly handles it.
#[tokio::test]
async fn client_service_handles_exited_heartbeat_task() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build().finish();

    harness.stop_heartbeat_task().await;

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}

/// Force the heartbeat background task to panic, and check if the `Client` properly handles it.
#[tokio::test]
async fn client_service_handles_panicked_heartbeat_task() {
    zebra_test::init();

    let (mut client, mut harness) = ClientTestHarness::build()
        .with_heartbeat_task(async move {
            panic!("heartbeat task failure");
        })
        .finish();

    // Allow the custom heartbeat task to run.
    tokio::task::yield_now().await;

    assert!(client.is_failed().await);
    assert!(harness.current_error().is_some());
    assert!(!harness.wants_connection_heartbeats());
    assert!(harness.try_to_receive_outbound_client_request().is_closed());
}
