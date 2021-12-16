//! Fixed peer [`Client`] test vectors.

use zebra_test::service_extensions::IsReady;

use crate::{peer::MockedClientHandle, protocol::external::types::Version, PeerError};

#[tokio::test]
async fn client_service_ready_ok() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    assert!(client.is_ready());
    assert!(handle.current_error().is_none());
    assert!(handle.is_connected());
    assert!(handle.try_to_receive_request().is_empty());
}

#[tokio::test]
async fn client_service_ready_heartbeat_exit() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    handle.set_error(PeerError::HeartbeatTaskExited);
    handle.drop_shutdown_receiver();

    assert!(client.not_ready_due_to_error());
    assert!(handle.current_error().is_some());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_request_drop() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    handle.set_error(PeerError::ConnectionDropped);
    handle.drop_request_receiver();

    assert!(client.not_ready_due_to_error());
    assert!(handle.current_error().is_some());
    assert!(!handle.is_connected());
}

#[tokio::test]
async fn client_service_ready_request_close() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    handle.set_error(PeerError::ConnectionClosed);
    handle.close_request_receiver();

    assert!(client.not_ready_due_to_error());
    assert!(handle.current_error().is_some());
    assert!(!handle.is_connected());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_error_in_slot() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    handle.set_error(PeerError::Overloaded);

    assert!(client.not_ready_due_to_error());
    assert!(handle.current_error().is_some());
    assert!(!handle.is_connected());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_ready_multiple_errors() {
    zebra_test::init();

    let (mut handle, mut client) = MockedClientHandle::new(Version(0));

    handle.set_error(PeerError::DuplicateHandshake);
    handle.drop_shutdown_receiver();
    handle.close_request_receiver();

    assert!(client.not_ready_due_to_error());
    assert!(handle.current_error().is_some());
    assert!(handle.try_to_receive_request().is_closed());
}

#[tokio::test]
async fn client_service_drop_cleanup() {
    zebra_test::init();

    let (mut handle, client) = MockedClientHandle::new(Version(0));

    std::mem::drop(client);

    assert!(handle.current_error().is_some());
    assert!(!handle.is_connected());
    assert!(handle.try_to_receive_request().is_closed());
}
