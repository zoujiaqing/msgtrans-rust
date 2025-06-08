pub mod api;
pub mod config;
pub mod core;
pub mod pool;

// 重新导出核心API (使用api模块的实现)
pub use api::{
    Transport, TransportBuilder, ConnectionManager, ServerManager
};

// 重新导出配置和其他核心类型
pub use config::TransportConfig;
pub use core::{
    Session, Protocol,
    ConnectionConfig, TcpConfig, WebSocketConfig, QuicConfig,
    PoolConfig, PoolStatus,
    ProtocolAdapter, SessionInfo
};

// 重新导出智能池管理
pub use pool::{
    SmartConnectionPool, ExpansionStrategy, PoolDetailedStatus,
    MemoryPool, MemoryPoolStatus, BufferSize,
    PerformanceMetrics
}; 