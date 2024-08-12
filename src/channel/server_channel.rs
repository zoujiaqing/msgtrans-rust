use crate::callbacks::{
    OnMessageHandler, OnServerConnectHandler, OnServerDisconnectHandler, OnServerErrorHandler,
    OnServerTimeoutHandler,
};
use crate::packet::Packet;
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
        message_handler: Option<OnMessageHandler>,
        on_connect: Option<OnServerConnectHandler>,
        on_disconnect: Option<OnServerDisconnectHandler>,
        on_error: Option<OnServerErrorHandler>,
        on_timeout: Option<OnServerTimeoutHandler>,
    );
}