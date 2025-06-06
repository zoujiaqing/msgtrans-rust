/// 协议适配器实现模块
/// 
/// 此模块包含各种传输协议的具体适配器实现

pub mod tcp;
pub mod websocket;
pub mod quic;
pub mod factories;

pub use tcp::*;
pub use quic::*;
pub use websocket::*;
pub use factories::{create_standard_registry, TcpFactory, TcpConnection, TcpServerWrapper};
pub use factories::{WebSocketFactory, WebSocketConnection, WebSocketServerWrapper};
pub use factories::{QuicFactory, QuicConnection, QuicServerWrapper}; 