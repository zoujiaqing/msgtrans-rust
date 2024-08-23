use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
    OnServerTimeoutHandler,
};
use crate::channel::ServerChannel;
use crate::session::TransportSession;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MessageTransportServer {
    channels: Arc<Mutex<Vec<Arc<Mutex<dyn ServerChannel + Send + Sync>>>>>,
    sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
    next_id: Arc<AtomicUsize>,
    message_handler: Option<OnMessageHandler>,
    connect_handler: Option<OnServerConnectHandler>,
    disconnect_handler: Option<OnServerDisconnectHandler>,
    error_handler: Option<OnServerErrorHandler>,
    timeout_handler: Option<OnServerTimeoutHandler>,
}

impl MessageTransportServer {
    pub fn new() -> Self {
        MessageTransportServer {
            channels: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicUsize::new(1)),
            message_handler: None,
            connect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            timeout_handler: None,
        }
    }

    pub async fn start(&self) {
        let channels_vec = {
            let channels_lock = self.channels.lock().await;
            channels_lock.clone()
        };

        for channel in channels_vec.into_iter() {
            let sessions_clone = Arc::clone(&self.sessions);
            let next_id_clone = Arc::clone(&self.next_id);
            let message_handler = self.message_handler.clone();
            let connect_handler = self.connect_handler.clone();
            let disconnect_handler = self.disconnect_handler.clone();
            let error_handler = self.error_handler.clone();

            tokio::spawn(async move {
                let mut channel = channel.lock().await;
                channel
                    .start(
                        sessions_clone,
                        next_id_clone,
                        message_handler,
                        connect_handler,
                        disconnect_handler,
                        error_handler,
                    )
                    .await;
            });
        }
    }

    pub async fn add_channel(&self, channel: Arc<Mutex<dyn ServerChannel + Send + Sync>>) {
        let mut channels_lock = self.channels.lock().await;
        channels_lock.push(channel);
    }

    pub async fn set_message_handler(&mut self, handler: OnMessageHandler) {
        let mut sessions_lock = self.sessions.lock().await;
        for session in sessions_lock.values_mut() {
            let session_guard = session.clone();
            session_guard.set_message_handler(handler.clone()).await;
        }
        self.message_handler = Some(handler);
    }

    pub fn set_connect_handler(&mut self, handler: OnServerConnectHandler) {
        self.connect_handler = Some(handler);
    }

    pub fn set_disconnect_handler(&mut self, handler: OnServerDisconnectHandler) {
        self.disconnect_handler = Some(handler);
    }

    pub fn set_error_handler(&mut self, handler: OnServerErrorHandler) {
        self.error_handler = Some(handler);
    }

    pub fn set_timeout_handler_handler(&mut self, handler: OnServerTimeoutHandler) {
        self.timeout_handler = Some(handler);
    }
}
