/// 统一架构模块
/// 
/// 这个模块实现了 msgtrans 的新统一架构，基于以下设计原则：
/// 1. 单一Actor模式
/// 2. 泛型抽象
/// 3. Stream + async/await，消除回调
/// 4. 类型安全的事件系统

pub mod packet;
pub mod event;
pub mod command;
pub mod actor;
pub mod adapter;
pub mod adapters;
pub mod stream;
pub mod config;
pub mod error;
pub mod api;
pub mod protocol;
pub mod protocol_adapter;

// 重新导出核心类型
pub use packet::{UnifiedPacket, PacketType, PacketError};
pub use event::{TransportEvent, ProtocolEvent};
pub use command::{TransportCommand, ProtocolCommand, ConnectionInfo, TransportStats};
pub use actor::{GenericActor, ActorState, ActorHandle, ActorManager};
pub use adapter::{ProtocolAdapter, AdapterStats, ProtocolConfig, ConfigError};
pub use adapters::{TcpAdapter, WebSocketAdapter, QuicAdapter};
pub use stream::{EventStream, GenericReceiver};
pub use config::{ConfigBuilder, TransportConfig, GlobalConfig};
pub use error::{TransportError, RecoveryStrategy, ErrorHandler};
pub use api::{Transport, TransportBuilder, ConnectionManager, ServerManager};
pub use protocol::{ProtocolFactory, ProtocolRegistry, Connection, Server, ProtocolConfig as ProtocolConfigTrait};
pub use protocol_adapter::ProtocolConnectionAdapter;

// 核心类型别名
pub type SessionId = u64;
pub type PacketId = u64;

/// 关闭原因枚举
#[derive(Debug, Clone, PartialEq)]
pub enum CloseReason {
    /// 正常关闭
    Normal,
    /// 错误导致的关闭
    Error(String),
    /// 超时关闭
    Timeout,
    /// 远程关闭
    RemoteClosed,
    /// 本地关闭
    LocalClosed,
} 