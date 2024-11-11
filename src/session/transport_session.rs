use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;

#[async_trait]
pub trait TransportSession: Send + Sync {
    async fn send(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn close(self: Arc<Self>);
    fn id(&self) -> usize;

    async fn set_message_handler(self: Arc<Self>, handler: Arc<Mutex<OnMessageHandler>>);
    async fn get_message_handler(&self) -> Option<Arc<Mutex<OnMessageHandler>>>;

    async fn set_close_handler(self: Arc<Self>, handler: Arc<Mutex<OnCloseHandler>>);
    async fn get_close_handler(&self) -> Option<Arc<Mutex<OnCloseHandler>>>;

    async fn set_error_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionErrorHandler>>);
    async fn get_error_handler(&self) -> Option<Arc<Mutex<OnSessionErrorHandler>>>;

    async fn set_timeout_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionTimeoutHandler>>);
    async fn get_timeout_handler(&self) -> Option<Arc<Mutex<OnSessionTimeoutHandler>>>;

    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}