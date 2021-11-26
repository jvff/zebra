use std::task::{Context, Poll};

use tower::{
    load::{Load, PeakEwma},
    Service,
};

use crate::peer::Client;

/// A client service wrapper that keeps track of its load.
pub struct LoadTrackedClient {
    service: PeakEwma<Client>,
}

impl LoadTrackedClient {
    /// Create a new [`LoadTrackedClient`] wrapping the provided `service`.
    pub fn new(service: PeakEwma<Client>) -> Self {
        LoadTrackedClient { service }
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
