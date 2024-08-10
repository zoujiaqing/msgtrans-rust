use crate::packet::Packet;
use crate::session::TransportSession;
use crossbeam::channel::Sender;
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[async_trait::async_trait]
pub trait ServerChannel: Send + Sync {
    async fn start(
        &mut self,
        sessions: Arc<RwLock<HashMap<usize, Arc<RwLock<dyn TransportSession + Send + Sync>>>>>,
        next_id: Arc<AtomicUsize>
    );
}