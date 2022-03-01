//! Zebra supported RPC methods.
//!
//! Based on the [`zcashd` RPC methods](https://zcash.github.io/rpc/)
//! as used by `lightwalletd.`
//!
//! Some parts of the `zcashd` RPC documentation are outdated.
//! So this implementation follows the `lightwalletd` client implementation.

use jsonrpc_core::{self, Result};
use jsonrpc_derive::rpc;

use zebra_network::constants::USER_AGENT;
use zebra_node_services::{mempool, BoxError};

#[cfg(test)]
mod tests;

#[rpc(server)]
/// RPC method signatures.
pub trait Rpc {
    /// getinfo
    ///
    /// Returns software information from the RPC server running Zebra.
    ///
    /// zcashd reference: <https://zcash.github.io/rpc/getinfo.html>
    ///
    /// Result:
    /// {
    ///      "build": String, // Full application version
    ///      "subversion", String, // Zebra user agent
    /// }
    ///
    /// Note 1: We only expose 2 fields as they are the only ones needed for
    /// lightwalletd: <https://github.com/zcash/lightwalletd/blob/v0.4.9/common/common.go#L91-L95>
    ///
    /// Note 2: <https://zcash.github.io/rpc/getinfo.html> is outdated so it does not
    /// show the fields we are exposing. However, this fields are part of the output
    /// as shown in the following zcashd code:
    /// <https://github.com/zcash/zcash/blob/v4.6.0-1/src/rpc/misc.cpp#L86-L87>
    /// Zcash open ticket to add this fields to the docs: <https://github.com/zcash/zcash/issues/5606>
    #[rpc(name = "getinfo")]
    fn get_info(&self) -> Result<GetInfo>;

    /// getblockchaininfo
    ///
    /// TODO: explain what the method does
    ///       link to the zcashd RPC reference
    ///       list the arguments and fields that lightwalletd uses
    ///       note any other lightwalletd changes
    #[rpc(name = "getblockchaininfo")]
    fn get_blockchain_info(&self) -> Result<GetBlockChainInfo>;

    /// Send a raw signed transaction.
    ///
    /// Sends the raw bytes of a signed transaction to the network, if the transaction is valid.
    ///
    /// See Zcashd's RPC
    /// [`sendrawtransaction`](https://zcash.github.io/rpc/sendrawtransaction.html) documentation
    /// for more information.
    #[rpc(name = "sendrawtransaction")]
    fn send_raw_transaction(&self, raw_transaction_hex: String) -> Result<SentTransactionHash>;
}

/// RPC method implementations.
pub struct RpcImpl<Mempool> {
    /// Zebra's application version.
    app_version: String,

    /// A handle to the mempool service.
    ///
    /// Used when sending raw transactions.
    mempool: Mempool,
}

impl<Mempool> RpcImpl<Mempool>
where
    Mempool: tower::Service<mempool::Request, Response = mempool::Response, Error = BoxError>
        + Send
        + Sync
        + 'static,
{
    /// Create a new instance of the RPC handler.
    pub fn new(app_version: String, mempool: Mempool) -> Self {
        RpcImpl {
            app_version,
            mempool,
        }
    }
}

impl<Mempool> Rpc for RpcImpl<Mempool>
where
    Mempool: tower::Service<mempool::Request, Response = mempool::Response, Error = BoxError>
        + Send
        + Sync
        + 'static,
{
    fn get_info(&self) -> Result<GetInfo> {
        let response = GetInfo {
            build: self.app_version.clone(),
            subversion: USER_AGENT.into(),
        };

        Ok(response)
    }

    fn get_blockchain_info(&self) -> Result<GetBlockChainInfo> {
        // TODO: dummy output data, fix in the context of #3143
        let response = GetBlockChainInfo {
            chain: "TODO: main".to_string(),
        };

        Ok(response)
    }

    fn send_raw_transaction(&self, raw_transaction_hex: String) -> Result<SentTransactionHash> {
        todo!();
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
/// Response to a `getinfo` RPC request.
pub struct GetInfo {
    build: String,
    subversion: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
/// Response to a `getblockchaininfo` RPC request.
pub struct GetBlockChainInfo {
    chain: String,
    // TODO: add other fields used by lightwalletd (#3143)
}

#[derive(serde::Serialize, serde::Deserialize)]
/// Response to a `sendrawtransaction` RPC request.
///
/// A JSON string with the transaction hash in hexadecimal.
pub struct SentTransactionHash(String);
