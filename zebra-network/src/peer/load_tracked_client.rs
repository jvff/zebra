use std::task::{Context, Poll};

use tower::{
    load::{Load, PeakEwma},
    Service,
};

use crate::{peer::Client, protocol::external::types::Version};

/// A client service wrapper that keeps track of its load.
///
/// It also keeps track of the peer's reported protocol version.
pub struct LoadTrackedClient {
    service: PeakEwma<Client>,
    version: Version,
}

impl LoadTrackedClient {
    /// Create a new [`LoadTrackedClient`] wrapping the provided `service`.
    pub fn new(service: PeakEwma<Client>, version: Version) -> Self {
        LoadTrackedClient { service, version }
    }

    /// Retrieve the peer's reported protocol version.
    pub fn version(&self) -> Version {
        self.version
    }
}

impl<Request> Service<Request> for LoadTrackedClient
where
    Client: Service<Request>,
{
    type Response = <Client as Service<Request>>::Response;
    type Error = <Client as Service<Request>>::Error;
    type Future = <PeakEwma<Client> as Service<Request>>::Future;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(context)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        self.service.call(request)
    }
}

impl Load for LoadTrackedClient {
    type Metric = <PeakEwma<Client> as Load>::Metric;

    fn load(&self) -> Self::Metric {
        self.service.load()
    }
}
