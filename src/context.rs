use std::sync::Arc;
use crate::session::TransportSession;

pub struct Context {
    session: Arc<dyn TransportSession + Send + Sync>,
}

impl Context {
    pub fn new(session: Arc<dyn TransportSession + Send + Sync>) -> Self {
        Context { session }
    }

    pub fn session(&self) -> Arc<dyn TransportSession + Send + Sync> {
        Arc::clone(&self.session)
    }
}