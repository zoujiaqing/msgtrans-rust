use crate::channel::ServerChannel;
use crate::packet::Packet;
use crate::session::TransportSession;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MessageTransportServer {
    channels: Arc<Mutex<Vec<Arc<Mutex<dyn ServerChannel + Send + Sync>>>>>,
    sessions: Arc<Mutex<HashMap<usize, Arc<Mutex<dyn TransportSession + Send + Sync>>>>>,
    next_id: Arc<AtomicUsize>,
}

impl MessageTransportServer {
    pub fn new() -> Self {
        MessageTransportServer {
            channels: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicUsize::new(1)),
        }
    }

    pub async fn start(self) {
        let channels = Arc::clone(&self.channels);
        let channels_vec = {
            let channels_lock = channels.lock().await;
            channels_lock.clone()
        };

        for channel in channels_vec.into_iter() {
            let channel_clone = Arc::clone(&channel);
            let sessions_clone = Arc::clone(&self.sessions);
            let next_id_clone = Arc::clone(&self.next_id);

            let mut channel_guard = channel_clone.lock().await;
            channel_guard.start(sessions_clone, next_id_clone).await;
        }
    }

    pub async fn add_channel(&mut self, channel: Arc<Mutex<dyn ServerChannel + Send + Sync>>) {
        let mut channels_lock = self.channels.lock().await;
        channels_lock.push(channel);
    }

    pub async fn set_message_handler(
        &mut self,
        handler: Arc<
            Mutex<
                Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>,
            >,
        >,
    ) {
        let sessions = Arc::clone(&self.sessions);
        let sessions_lock = sessions.lock().await;
        for session in sessions_lock.values() {
            let mut session_guard = session.lock().await;
            session_guard.set_message_handler(handler.clone());
        }
    }
}
