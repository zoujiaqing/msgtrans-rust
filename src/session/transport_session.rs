use crate::callbacks::{
    OnMessageHandler, OnReceiveHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::packet::Packet;
use crate::context::Context;
use std::sync::Arc;
use async_trait::async_trait;

#[async_trait]
pub trait TransportSession: Send + Sync {
    async fn send_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn receive_packet(self: Arc<Self>) -> Option<Packet>;
    async fn process_packet(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn close_session(self: Arc<Self>, context: Arc<Context>);
    fn id(&self) -> usize;

    async fn set_message_handler(self: Arc<Self>, handler: OnMessageHandler) ;
    async fn get_message_handler(&self) -> Option<OnMessageHandler>;

    async fn set_receive_handler(self: Arc<Self>, handler: OnReceiveHandler);
    async fn get_receive_handler(&self) -> Option<OnReceiveHandler>;

    async fn set_close_handler(self: Arc<Self>, handler: OnCloseHandler);
    async fn get_close_handler(&self) -> Option<OnCloseHandler>;

    async fn set_error_handler(self: Arc<Self>, handler: OnSessionErrorHandler);
    async fn get_error_handler(&self) -> Option<OnSessionErrorHandler>;

    async fn set_timeout_handler(self: Arc<Self>, handler: OnSessionTimeoutHandler);
    async fn get_timeout_handler(&self) -> Option<OnSessionTimeoutHandler>;
}