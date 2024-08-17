use crate::packet::Packet;
use async_trait::async_trait;
use crate::callbacks::{OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler};

#[async_trait]
pub trait ClientChannel: Send + Sync {
    fn set_reconnect_handler(&mut self, handler: OnReconnectHandler);
    fn set_disconnect_handler(&mut self, handler: OnClientDisconnectHandler);
    fn set_error_handler(&mut self, handler: OnClientErrorHandler);
    fn set_send_handler(&mut self, handler: OnSendHandler);
    fn set_on_message_handler(&mut self, handler: OnClientMessageHandler);

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    
    // 修改后的接口，用于替换 receive 方法
    async fn start_receiving(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}