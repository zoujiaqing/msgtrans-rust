pub mod server;
pub mod client;
pub mod channel;
pub mod context;
pub mod packet;
pub mod session;
pub mod callbacks;
pub mod compression;
pub mod transport;
pub mod new_client;

// 重新导出新架构的核心类型
pub use transport::{TcpConnection, QuicConnection, WebSocketConnection};
pub use new_client::TransportClient;