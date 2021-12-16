//! Extension traits for [`Service`] types to help with testing.

use futures::FutureExt;
use tower::{Service, ServiceExt};

/// An extension trait to check if a [`Service`] is immediately ready to be called.
pub trait IsReady<Request>: Service<Request> {
    /// Check if the [`Service`] is immediately ready to be called.
    fn is_ready(&mut self) -> bool;

    /// Check if the [`Service`] is not immediately ready because it returns an error.
    fn not_ready_due_to_error(&mut self) -> bool;
}

impl<S, Request> IsReady<Request> for S
where
    S: Service<Request>,
{
    fn is_ready(&mut self) -> bool {
        matches!(self.ready().now_or_never(), Some(Ok(_)))
    }

    fn not_ready_due_to_error(&mut self) -> bool {
        matches!(self.ready().now_or_never(), Some(Err(_)))
    }
}
