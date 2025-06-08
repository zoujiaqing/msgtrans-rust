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

// 新增模块
pub mod connection;
pub mod discovery;
pub mod plugin;

// 类型定义
pub type PacketId = u32;

/// 会话ID的类型安全包装器
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SessionId(u64);

impl SessionId {
    /// 创建新的会话ID
    pub fn new(id: u64) -> Self {
        Self(id)
    }
    
    /// 获取原始ID值
    pub fn as_u64(&self) -> u64 {
        self.0
    }
    
    /// 生成下一个会话ID
    pub fn next(&self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "session-{}", self.0)
    }
}

impl From<u64> for SessionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<SessionId> for u64 {
    fn from(session_id: SessionId) -> Self {
        session_id.0
    }
}

// 重新导出核心类型
pub use packet::{Packet, PacketType, PacketError};
pub use transport::{
    Transport, 
    TransportBuilder as Builder, 
    TransportConfig as Config,
    ConnectionPool,
    MemoryPool
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

// 重新导出新的抽象
pub use connection::{Connection, Server, ConnectionFactory};
pub use discovery::{ServiceDiscovery, ServiceInstance, LoadBalancer, LoadBalanceStrategy};
pub use plugin::{ProtocolPlugin, PluginManager, PluginInfo};

// 便捷的类型别名
pub type Result<T> = std::result::Result<T, TransportError>;