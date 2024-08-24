use std::sync::Arc;
use crate::packet::Packet;
use crate::context::Context;

// Session-specific callbacks
pub type OnMessageHandler = dyn Fn(Arc<Context>, Packet) + Send + Sync;
pub type OnReceiveHandler = dyn Fn(Arc<Context>, Packet) + Send + Sync;
pub type OnCloseHandler = dyn Fn(Arc<Context>) + Send + Sync;
pub type OnSessionErrorHandler = dyn Fn(Arc<Context>, Box<dyn std::error::Error + Send + Sync>) + Send + Sync;
pub type OnSessionTimeoutHandler = dyn Fn(Arc<Context>) + Send + Sync;

// Server-specific callbacks
pub type OnServerConnectHandler = dyn Fn(Arc<Context>) + Send + Sync;
pub type OnServerDisconnectHandler = dyn Fn(Arc<Context>) + Send + Sync;
pub type OnServerErrorHandler = dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync;
pub type OnServerTimeoutHandler = dyn Fn(Arc<Context>) + Send + Sync;

// Client-specific callbacks
pub type OnReconnectHandler = dyn Fn() + Send + Sync;
pub type OnClientDisconnectHandler = dyn Fn() + Send + Sync;
pub type OnClientErrorHandler = dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync;
pub type OnSendHandler = dyn Fn(Packet, Result<(), Box<dyn std::error::Error + Send + Sync>>) + Send + Sync;
pub type OnClientMessageHandler = dyn Fn(Packet) + Send + Sync;
