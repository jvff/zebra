use crate::protocol::external::types::Version;

pub(super) struct ReadyService<S> {
    service: S,
    version: Version,
}

impl<S> ReadyService<S> {
    pub fn new(service: S, version: Version) -> Self {
        ReadyService { service, version }
    }
}
