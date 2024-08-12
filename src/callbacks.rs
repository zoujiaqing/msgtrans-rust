use std::sync::Arc;
use tokio::sync::Mutex;
use crate::packet::Packet;
use crate::context::Context;

// Session-specific callbacks
pub type OnMessageHandler = Arc<Mutex<dyn Fn(Arc<Context>, Packet) + Send + Sync>>;
pub type OnReceiveHandler = Arc<Mutex<dyn Fn(Arc<Context>, Packet) + Send + Sync>>;
pub type OnCloseHandler = Arc<Mutex<dyn Fn(Arc<Context>) + Send + Sync>>;
pub type OnSessionErrorHandler = Arc<Mutex<dyn Fn(Arc<Context>, Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>;
pub type OnSessionTimeoutHandler = Arc<Mutex<dyn Fn(Arc<Context>) + Send + Sync>>;

// Server-specific callbacks
pub type OnServerConnectHandler = Arc<Mutex<dyn Fn(Arc<Context>) + Send + Sync>>;
pub type OnServerDisconnectHandler = Arc<Mutex<dyn Fn(Arc<Context>) + Send + Sync>>;
pub type OnServerErrorHandler = Arc<Mutex<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>;
pub type OnServerTimeoutHandler = Arc<Mutex<dyn Fn(Arc<Context>) + Send + Sync>>;

// Client-specific callbacks
pub type OnReconnectHandler = Arc<Mutex<dyn Fn() + Send + Sync>>;
pub type OnClientDisconnectHandler = Arc<Mutex<dyn Fn() + Send + Sync>>;
pub type OnClientErrorHandler = Arc<Mutex<dyn Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync>>;
pub type OnSendHandler = Arc<Mutex<dyn Fn(Packet) + Send + Sync>>;