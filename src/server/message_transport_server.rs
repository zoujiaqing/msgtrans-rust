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
use crate::packet::Packet;
use crate::context::Context;

pub struct MessageTransportServer {
    channels: Arc<Mutex<Vec<Arc<Mutex<dyn ServerChannel + Send + Sync>>>>>,
    sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
    next_id: Arc<AtomicUsize>,
    message_handler: Option<Arc<Mutex<OnMessageHandler>>>,
    connect_handler: Option<Arc<Mutex<OnServerConnectHandler>>>,
    disconnect_handler: Option<Arc<Mutex<OnServerDisconnectHandler>>>,
    error_handler: Option<Arc<Mutex<OnServerErrorHandler>>>,
    timeout_handler: Option<Arc<Mutex<OnServerTimeoutHandler>>>,
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

    pub async fn shutdown(&mut self) {
        let sessions = self.sessions.lock().await;
        let sessions_cloned: Vec<_> = sessions.iter().map(|(_, session)| Arc::clone(session)).collect();
    
        for session in sessions_cloned {
            session.close().await;
        }
    
        self.sessions.lock().await.clear();
        
        let mut channels_lock = self.channels.lock().await;
    
        for channel in channels_lock.iter_mut() {
            let mut channel_lock = channel.lock().await;
            channel_lock.shutdown().await;
        }
    }

    pub async fn add_channel<T>(&self, channel: T)
    where
        T: ServerChannel + Send + Sync + 'static,
    {
        let mut channels_lock = self.channels.lock().await;
        channels_lock.push(Arc::new(Mutex::new(channel)));
    }

    pub async fn set_message_handler<F>(&mut self, handler: F)
    where
        F: Fn(Arc<Context>, Packet) + Send + Sync + 'static,
    {
        let handler_arc = Arc::new(Mutex::new(handler));

        let sessions_lock = self.sessions.lock().await;
        for session in sessions_lock.values() {
            let handler_clone = Arc::clone(&handler_arc);
            let session_clone = Arc::clone(session);
            session_clone.set_message_handler(handler_clone).await;
        }
        self.message_handler = Some(handler_arc);
    }

    pub fn set_connect_handler<F>(&mut self, handler: F)
    where
        F: Fn(Arc<Context>) + Send + Sync + 'static,
    {
        self.connect_handler = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_disconnect_handler<F>(&mut self, handler: F)
    where
        F: Fn(Arc<Context>) + Send + Sync + 'static,
    {
        self.disconnect_handler = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_error_handler<F>(&mut self, handler: F)
    where
        F: Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync + 'static,
    {
        self.error_handler = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_timeout_handler<F>(&mut self, handler: F)
    where
        F: Fn(Arc<Context>) + Send + Sync + 'static,
    {
        self.timeout_handler = Some(Arc::new(Mutex::new(handler)));
    }
}