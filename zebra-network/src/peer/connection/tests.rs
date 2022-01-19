//! Tests for peer connections

use futures::channel::mpsc;
use tokio::io::{duplex, DuplexStream};
use tokio_util::codec::{FramedRead, FramedWrite};

use zebra_chain::parameters::Network;
use zebra_test::mock_service::MockService;

use crate::{
    peer::{
        client::ClientRequestReceiver, connection::State, ClientRequest, Connection, ErrorSlot,
    },
    peer_set::ActiveConnectionCounter,
    protocol::external::Codec,
    Request, Response,
};

mod prop;
mod vectors;

/// Creates a new [`Connection`] instance for testing.
fn new_test_connection<A>() -> (
    Connection<MockService<Request, Response, A>, FramedWrite<DuplexStream, Codec>>,
    mpsc::Sender<ClientRequest>,
    MockService<Request, Response, A>,
    FramedRead<DuplexStream, Codec>,
    ErrorSlot,
) {
    let (client_tx, client_rx) = mpsc::channel(1);
    let (peer_outbound_writer, peer_outbound_reader) = duplex(4096);

    let codec = Codec::builder()
        .for_network(Network::Mainnet)
        .with_metrics_addr_label("test".into())
        .finish();
    let peer_outbound_tx = FramedWrite::new(peer_outbound_writer, codec.clone());
    let peer_outbound_rx = FramedRead::new(peer_outbound_reader, codec);

    let mock_inbound_service = MockService::build().finish();

    let shared_error_slot = ErrorSlot::default();

    let connection = Connection {
        state: State::AwaitingRequest,
        request_timer: None,
        cached_addrs: Vec::new(),
        svc: mock_inbound_service.clone(),
        client_rx: ClientRequestReceiver::from(client_rx),
        error_slot: shared_error_slot.clone(),
        peer_tx: peer_outbound_tx,
        connection_tracker: ActiveConnectionCounter::new_counter().track_connection(),
        metrics_label: "test".to_string(),
        last_metrics_state: None,
    };

    (
        connection,
        client_tx,
        mock_inbound_service,
        peer_outbound_rx,
        shared_error_slot,
    )
}
