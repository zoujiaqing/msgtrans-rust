use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
};
use crate::session::TransportSession;
use std::collections::HashMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::Mutex;

#[async_trait::async_trait]
pub trait ServerChannel: Send + Sync {
    async fn start(
        &mut self,
        sessions: Arc<Mutex<HashMap<usize, Arc<dyn TransportSession + Send + Sync>>>>,
        next_id: Arc<AtomicUsize>,
        message_handler: Option<Arc<Mutex<OnMessageHandler>>>,
        connect_handler: Option<Arc<Mutex<OnServerConnectHandler>>>,
        disconnect_handler: Option<Arc<Mutex<OnServerDisconnectHandler>>>,
        error_handler: Option<Arc<Mutex<OnServerErrorHandler>>>
    );
    async fn shutdown(&mut self);
}