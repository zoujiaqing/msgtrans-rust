use crate::packet::Packet;
use async_trait::async_trait;
use crate::callbacks::{OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler};
use std::sync::Arc;
use tokio::sync::Mutex;
#[async_trait]
pub trait ClientChannel: Send + Sync {
    fn set_reconnect_handler(&mut self, handler: Arc<Mutex<OnReconnectHandler>>);
    fn set_disconnect_handler(&mut self, handler: Arc<Mutex<OnClientDisconnectHandler>>);
    fn set_error_handler(&mut self, handler: Arc<Mutex<OnClientErrorHandler>>);
    fn set_send_handler(&mut self, handler: Arc<Mutex<OnSendHandler>>);
    fn set_message_handler(&mut self, handler: Arc<Mutex<OnClientMessageHandler>>);

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}