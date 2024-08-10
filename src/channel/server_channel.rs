use crate::packet::Packet;
use crate::session::TransportSession;
use crossbeam::channel::Sender;
use std::sync::{Arc};
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[async_trait::async_trait]
pub trait ServerChannel: Send + Sync {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<Mutex<dyn TransportSession + Send + Sync>>>>>,
        next_id: Arc<AtomicUsize>
    );
}