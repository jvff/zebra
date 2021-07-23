use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{Context, Poll},
};

use futures::prelude::*;
use tokio::net::TcpStream;
use tower::{discover::Change, Service, ServiceExt};
use tracing_futures::Instrument;

use zebra_chain::best_tip_height::BestTipHeight;

use crate::{BoxError, Request, Response};

use super::{Client, ConnectedAddr, Handshake};

/// A wrapper around [`peer::Handshake`] that opens a TCP connection before
/// forwarding to the inner handshake service. Writing this as its own
/// [`tower::Service`] lets us apply unified timeout policies, etc.
pub struct Connector<S, B> {
    handshaker: Handshake<S, B>,
}

impl<S: Clone, B: Clone> Clone for Connector<S, B> {
    fn clone(&self) -> Self {
        Connector {
            handshaker: self.handshaker.clone(),
        }
    }
}

impl<S, B> Connector<S, B> {
    pub fn new(handshaker: Handshake<S, B>) -> Self {
        Connector { handshaker }
    }
}

impl<S, B> Service<SocketAddr> for Connector<S, B>
where
    S: Service<Request, Response = Response, Error = BoxError> + Clone + Send + 'static,
    S::Future: Send,
    B: BestTipHeight + Clone + Send + 'static,
{
    type Response = Change<SocketAddr, Client>;
    type Error = BoxError;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, addr: SocketAddr) -> Self::Future {
        let mut hs = self.handshaker.clone();
        let connected_addr = ConnectedAddr::new_outbound_direct(addr);
        let connector_span = info_span!("connector", peer = ?connected_addr);
        async move {
            let stream = TcpStream::connect(addr).await?;
            hs.ready_and().await?;
            let client = hs.call((stream, connected_addr)).await?;
            Ok(Change::Insert(addr, client))
        }
        .instrument(connector_span)
        .boxed()
    }
}
