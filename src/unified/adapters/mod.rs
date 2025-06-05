/// 协议适配器实现模块
/// 
/// 此模块包含各种传输协议的具体适配器实现

pub mod tcp;
pub mod websocket;
pub mod quic;
pub mod factories;

// 重新导出适配器类型
pub use tcp::TcpAdapter;
pub use websocket::WebSocketAdapter;
pub use quic::QuicAdapter;
pub use factories::{TcpFactory, WebSocketFactory, QuicFactory, create_standard_registry}; 