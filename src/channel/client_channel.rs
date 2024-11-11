use crate::packet::Packet;
use async_trait::async_trait;
use crate::callbacks::{OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler};
use std::sync::Arc;
use tokio::sync::Mutex;

#[async_trait]
pub trait ClientChannel: Send + Sync {
    fn set_reconnect_handler(&mut self, handler: Arc<OnReconnectHandler>);
    fn set_disconnect_handler(&mut self, handler: Arc<OnClientDisconnectHandler>);
    fn set_error_handler(&mut self, handler: Arc<OnClientErrorHandler>);
    fn set_send_handler(&mut self, handler: Arc<OnSendHandler>);
    fn set_message_handler(&mut self, handler: Arc<OnClientMessageHandler>);

    fn bind_local_addr(&mut self, ip_addr: String);

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}