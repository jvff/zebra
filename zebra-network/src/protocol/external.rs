//! Network protocol types and serialization for the Zcash wire format.

/// Node address wire formats.
mod addr;
/// A Tokio codec that transforms an `AsyncRead` into a `Stream` of `Message`s.
pub mod codec;
/// The type emitted by the [`Codec`] decoder, which is either a complete [`Message`] or a marker
/// that the message is still being downloaded.
mod decoder_output;
/// Inventory items.
mod inv;
/// An enum of all supported Bitcoin message types.
mod message;
/// Newtype wrappers for primitive types.
pub mod types;

#[cfg(any(test, feature = "proptest-impl"))]
pub mod arbitrary;
#[cfg(test)]
mod tests;

pub use addr::{canonical_socket_addr, AddrInVersion};
pub use codec::Codec;
pub use decoder_output::DecoderOutput;
pub use inv::InventoryHash;
pub use message::Message;
pub use zebra_chain::serialization::MAX_PROTOCOL_MESSAGE_LEN;
