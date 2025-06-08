pub mod api;
pub mod config;
pub mod core;
pub mod pool;

// 重新导出核心API
pub use core::{
    Transport, TransportBuilder, Session, Protocol,
    ConnectionConfig, TcpConfig, WebSocketConfig, QuicConfig,
    PoolConfig, PoolStatus, TransportConfig,
    ProtocolAdapter, SessionInfo
};

// 重新导出智能池管理  
pub use pool::{
    SmartConnectionPool, ExpansionStrategy, PoolDetailedStatus,
    MemoryPool, MemoryPoolStatus, BufferSize,
    PerformanceMetrics
}; 