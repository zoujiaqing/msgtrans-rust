use crate::channel::ServerChannel;
use crate::session::TransportSession;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::context::Context;
use crate::packet::Packet;
use crate::callbacks::{
    OnServerConnectHandler,
    OnServerDisconnectHandler,
    OnServerErrorHandler,
    OnServerTimeoutHandler,
    OnMessageHandler,
};

pub struct MessageTransportServer {
    channels: Arc<Mutex<Vec<Arc<Mutex<dyn ServerChannel + Send + Sync>>>>>,
    sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
    next_id: Arc<AtomicUsize>,
    message_handler: Option<OnMessageHandler>,
    on_connect: Option<OnServerConnectHandler>,
    on_disconnect: Option<OnServerDisconnectHandler>,
    on_error: Option<OnServerErrorHandler>,
    on_timeout: Option<OnServerTimeoutHandler>,
}

impl MessageTransportServer {
    pub fn new() -> Self {
        MessageTransportServer {
            channels: Arc::new(Mutex::new(Vec::new())),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(AtomicUsize::new(1)),
            message_handler: None,
            on_connect: None,
            on_disconnect: None,
            on_error: None,
            on_timeout: None,
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
            let on_connect = self.on_connect.clone();
            let on_disconnect = self.on_disconnect.clone();
            let on_error = self.on_error.clone();
            let on_timeout = self.on_timeout.clone();

            tokio::spawn(async move {
                let mut channel = channel.lock().await;
                channel.start(
                    sessions_clone,
                    next_id_clone,
                    message_handler,
                    on_connect,
                    on_disconnect,
                    on_error,
                    on_timeout,
                ).await;
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

    pub fn set_on_connect_handler(&mut self, handler: OnServerConnectHandler) {
        self.on_connect = Some(handler);
    }

    pub fn set_on_disconnect_handler(&mut self, handler: OnServerDisconnectHandler) {
        self.on_disconnect = Some(handler);
    }

    pub fn set_on_error_handler(&mut self, handler: OnServerErrorHandler) {
        self.on_error = Some(handler);
    }

    pub fn set_on_timeout_handler(&mut self, handler: OnServerTimeoutHandler) {
        self.on_timeout = Some(handler);
    }
}