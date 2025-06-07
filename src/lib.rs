/// msgtrans - 统一多协议传输库
/// 
/// 这是一个现代的、高性能的Rust传输库，提供TCP、WebSocket和QUIC协议的统一接口。
/// 基于Actor模式设计，完全消除回调地狱，提供类型安全的事件驱动API。

// 传输层
pub mod transport;

// 协议适配器
pub mod adapters;

// 协议抽象
pub mod protocol;

// 核心类型
pub mod packet;
pub mod event;
pub mod error;
pub mod actor;
pub mod command;
pub mod stream;

// 类型定义
pub type SessionId = u64;
pub type PacketId = u32;

// 重新导出核心类型
pub use packet::{Packet, PacketType, PacketError};
pub use transport::{
    Transport, 
    TransportBuilder as Builder, 
    TransportConfig as Config,
    GlobalConfig,
    ConnectionManager, 
    ServerManager
};
pub use event::TransportEvent as Event;
pub use stream::EventStream;
pub use error::{TransportError, CloseReason};
pub use command::{ConnectionInfo, TransportStats};

// 重新导出协议配置和新的traits
pub use protocol::{
    TcpConfig, WebSocketConfig, QuicConfig,
    ServerConfig, ClientConfig,  // 添加新的类型安全traits
};

// 便捷的类型别名
pub type Result<T> = std::result::Result<T, TransportError>;