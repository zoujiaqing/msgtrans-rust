pub mod api;
pub mod config;
pub mod core;
pub mod pool;
pub mod expert_config;

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
    ConnectionPool, ExpansionStrategy, PoolDetailedStatus,
    MemoryPool, MemoryPoolStatus, BufferSize,
    PerformanceMetrics
};

// 重新导出专家配置
pub use expert_config::{
    SmartPoolConfig, PerformanceConfig, ExpertConfig
};
