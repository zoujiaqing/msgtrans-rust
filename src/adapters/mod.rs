/// 协议适配器实现模块
/// 
/// 此模块包含各种传输协议的具体适配器实现

pub mod tcp;
pub mod websocket;
pub mod quic;
pub mod factories;

// 只导出用户真正需要的标准API（移除Builder类）
pub use tcp::{TcpAdapter, TcpError};
pub use websocket::{WebSocketAdapter, WebSocketError};
pub use quic::{QuicAdapter, QuicError};

// 导出工厂和连接类型
pub use factories::{create_standard_registry, TcpFactory, TcpConnection, TcpServerWrapper};
pub use factories::{WebSocketFactory, WebSocketConnection, WebSocketServerWrapper};
pub use factories::{QuicFactory, QuicConnection, QuicServerWrapper}; 