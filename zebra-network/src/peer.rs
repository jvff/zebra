//! Peer handling.

/// Handles outbound requests from our node to the network.
mod client;
/// The per-peer connection state machine.
mod connection;
/// Wrapper around handshake logic that also opens a TCP connection.
mod connector;
/// Peer-related errors.
mod error;
/// Performs peer handshakes.
mod handshake;
/// Groups some peer meta-data.
mod meta_data;

use client::{ClientRequest, ClientRequestReceiver, InProgressClientRequest, MustUseOneshotSender};

pub use client::Client;
pub use connection::Connection;
pub use connector::{Connector, OutboundConnectorRequest};
pub use error::{ErrorSlot, HandshakeError, PeerError, SharedPeerError};
pub use handshake::{ConnectedAddr, Handshake, HandshakeRequest};
pub use meta_data::PeerMetaData;
