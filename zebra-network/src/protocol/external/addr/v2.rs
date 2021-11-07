//! Zcash `addrv2` message node address serialization.
//!
//! Zebra parses received IPv4 and IPv6 addresses in the [`AddrV2`] format.
//! But it ignores all other address types.
//!
//! Zebra never sends `addrv2` messages, because peers still accept `addr` (v1) messages.

use std::{
    convert::TryInto,
    io::Read,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use byteorder::{BigEndian, ReadBytesExt};

use zebra_chain::serialization::{
    CompactSize64, DateTime32, SerializationError, TrustedPreallocate, ZcashDeserialize,
    ZcashDeserializeInto,
};

use crate::{
    meta_addr::MetaAddr,
    protocol::external::{types::PeerServices, MAX_PROTOCOL_MESSAGE_LEN},
};

#[cfg(any(test, feature = "proptest-impl"))]
use proptest_derive::Arbitrary;

/// The maximum permitted size of the `addr` field in `addrv2` messages.
///
/// > Field addr has a variable length, with a maximum of 512 bytes (4096 bits).
/// > Clients MUST reject messages with a longer addr field, irrespective of the network ID.
///
/// https://zips.z.cash/zip-0155#specification
pub const MAX_ADDR_V2_ADDR_SIZE: usize = 512;

/// The size of [`Ipv4Addr`]s in `addrv2` messages.
///
/// https://zips.z.cash/zip-0155#specification
pub const ADDR_V2_IPV4_ADDR_SIZE: usize = 4;

/// The size of [`Ipv6Addr`]s in `addrv2` messages.
///
/// https://zips.z.cash/zip-0155#specification
pub const ADDR_V2_IPV6_ADDR_SIZE: usize = 16;

/// The second format used for Bitcoin node addresses.
/// Contains a node address, its advertised services, and last-seen time.
/// This struct is serialized and deserialized into `addrv2` messages.
///
/// [ZIP 155](https://zips.z.cash/zip-0155#specification)
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[cfg_attr(any(test, feature = "proptest-impl"), derive(Arbitrary))]
pub(in super::super) enum AddrV2 {
    /// An IPv4 or IPv6 node address, in `addrv2` format.
    IpAddr {
        /// The unverified "last seen time" gossiped by the remote peer that sent us
        /// this address.
        ///
        /// See the [`MetaAddr::last_seen`] method for details.
        untrusted_last_seen: DateTime32,

        /// The unverified services for the peer at `ip_addr`:`port`.
        ///
        /// These services were advertised by the peer at that address,
        /// then gossiped via another peer.
        ///
        /// ## Security
        ///
        /// `untrusted_services` on gossiped peers may be invalid due to outdated
        /// records, older peer versions, or buggy or malicious peers.
        untrusted_services: PeerServices,

        /// The peer's IP address.
        ///
        /// Unlike [`AddrV1`], this can be an IPv4 or IPv6 address.
        ip: IpAddr,

        /// The peer's TCP port.
        port: u16,
    },

    /// A node address with an unimplemented `networkID`, in `addrv2` format.
    Unimplemented,
}

// > One message can contain up to 1,000 addresses.
// > Clients MUST reject messages with more addresses.

impl From<AddrV2> for Option<MetaAddr> {
    fn from(addr_v2: AddrV2) -> Self {
        if let AddrV2::IpAddr {
            untrusted_last_seen,
            untrusted_services,
            ip,
            port,
        } = addr_v2
        {
            let addr = SocketAddr::new(ip, port);

            Some(MetaAddr::new_gossiped_meta_addr(
                addr,
                untrusted_services,
                untrusted_last_seen,
            ))
        } else {
            None
        }
    }
}

/// Deserialize an `addrv2` entry according to:
/// https://zips.z.cash/zip-0155#specification
///
/// Unimplemented and unrecognised addresses are deserialized as `None`.
/// Deserialization consumes the bytes for these addresses.
impl ZcashDeserialize for AddrV2 {
    fn zcash_deserialize<R: Read>(mut reader: R) -> Result<Self, SerializationError> {
        // > uint32  Time that this node was last seen as connected to the network.
        let untrusted_last_seen = (&mut reader).zcash_deserialize_into()?;

        // > Service bits. A CompactSize-encoded bit field that is 64 bits wide.
        let untrusted_services: CompactSize64 = (&mut reader).zcash_deserialize_into()?;
        let untrusted_services = PeerServices::from_bits_truncate(untrusted_services.into());

        // > Network identifier. An 8-bit value that specifies which network is addressed.
        //
        // See the list of reserved network IDs in ZIP 155.
        let network_id = reader.read_u8()?;

        // > CompactSize      The length in bytes of addr.
        // > uint8[sizeAddr]  Network address. The interpretation depends on networkID.
        let addr: Vec<u8> = (&mut reader).zcash_deserialize_into()?;

        // > uint16  Network port. If not relevant for the network this MUST be 0.
        let port = reader.read_u16::<BigEndian>()?;

        if addr.len() > MAX_ADDR_V2_ADDR_SIZE {
            return Err(SerializationError::Parse(
                "addr field longer than MAX_ADDR_V2_ADDR_SIZE in addrv2 message",
            ));
        }

        if network_id == 0x01 {
            // > 0x01  IPV4  4   IPv4 address (globally routed internet)

            // > Clients MUST reject messages that contain addresses that have
            // > a different length than specified in this table for a specific network ID,
            // > as these are meaningless.
            if addr.len() != ADDR_V2_IPV4_ADDR_SIZE {
                return Err(SerializationError::Parse(
                    "IPv4 field length did not match ADDR_V2_IPV4_ADDR_SIZE in addrv2 message",
                ));
            }

            // > The IPV4 and IPV6 network IDs use addresses encoded in the usual way
            // > for binary IPv4 and IPv6 addresses in network byte order (big endian).
            let ip: [u8; ADDR_V2_IPV4_ADDR_SIZE] = addr.try_into().expect("just checked length");
            let ip = Ipv4Addr::from(ip);

            Ok(AddrV2::IpAddr {
                untrusted_last_seen,
                untrusted_services,
                ip: ip.into(),
                port,
            })
        } else if network_id == 0x02 {
            // > 0x02  IPV6  16  IPv6 address (globally routed internet)

            if addr.len() != ADDR_V2_IPV6_ADDR_SIZE {
                return Err(SerializationError::Parse(
                    "IPv6 field length did not match ADDR_V2_IPV6_ADDR_SIZE in addrv2 message",
                ));
            }

            let ip: [u8; ADDR_V2_IPV6_ADDR_SIZE] = addr.try_into().expect("just checked length");
            let ip = Ipv6Addr::from(ip);

            Ok(AddrV2::IpAddr {
                untrusted_last_seen,
                untrusted_services,
                ip: ip.into(),
                port,
            })
        } else {
            // unimplemented or unrecognised network ID, just consume the bytes
            //
            // > Clients MUST NOT gossip addresses from unknown networks,
            // > because they have no means to validate those addresses
            // > and so can be tricked to gossip invalid addresses.

            Ok(AddrV2::Unimplemented)
        }
    }
}

/// A serialized `addrv2` has:
/// * 4 byte time,
/// * 1-9 byte services,
/// * 1 byte networkID,
/// * 1-9 byte sizeAddr,
/// * 0-512 bytes addr,
/// * 2 bytes port.
#[allow(clippy::identity_op)]
pub(in super::super) const ADDR_V2_MIN_SIZE: usize = 4 + 1 + 1 + 1 + 0 + 2;

impl TrustedPreallocate for AddrV2 {
    fn max_allocation() -> u64 {
        // Since a maximal serialized Vec<AddrV2> uses at least three bytes for its length
        // (2MB  messages / 9B AddrV2 implies the maximal length is much greater than 253)
        // the max allocation can never exceed (MAX_PROTOCOL_MESSAGE_LEN - 3) / META_ADDR_SIZE
        ((MAX_PROTOCOL_MESSAGE_LEN - 3) / ADDR_V2_MIN_SIZE) as u64
    }
}
