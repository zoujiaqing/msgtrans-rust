/// 协议适配器实现模块
/// 
/// 此模块包含各种传输协议的具体适配器实现

pub mod tcp;
#[cfg(feature = "websocket")]
pub mod websocket;
#[cfg(feature = "quic")]
pub mod quic;
pub mod factories;

pub use tcp::*;
#[cfg(feature = "quic")]
pub use quic::*;
#[cfg(feature = "websocket")]
pub use websocket::*;
pub use factories::{create_standard_registry, TcpFactory, TcpConnection, TcpServerWrapper};
#[cfg(feature = "websocket")]
pub use factories::{WebSocketFactory, WebSocketConnection, WebSocketServerWrapper};
#[cfg(feature = "quic")]
pub use factories::{QuicFactory, QuicConnection, QuicServerWrapper}; 