use std::net::SocketAddr;

use crate::protocol::external::types::Version;

/// Meta-data extracted from a peer connection.
#[derive(Clone, Copy, Debug)]
pub struct PeerMetaData {
    /// The peer's address.
    address: SocketAddr,

    /// The peer's reported protocol version.
    version: Version,
}

impl PeerMetaData {
    /// Create a new [`PeerMetaData`] with the provided meta-data.
    pub fn new(address: SocketAddr, version: Version) -> Self {
        PeerMetaData { address, version }
    }

    /// Retrieve the peer's address.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Retrieve the peer's reported protocol version.
    pub fn version(&self) -> Version {
        self.version
    }
}

/// [`PartialEq`] and [`Eq`] can be used to see if two [`PeerMetaData`]s refer to the same peer.
///
/// This is required so that [`PeerMetaData`] can be used as a key type inside
/// [`tower::discover::Discover`].
///
/// # Correctness
///
/// Only the peer address is used in the comparison, so this should be used to check if the actual
/// meta-data is the same in two instances.
impl PartialEq for PeerMetaData {
    fn eq(&self, other: &Self) -> bool {
        self.address.eq(&other.address)
    }
}

impl Eq for PeerMetaData {}
