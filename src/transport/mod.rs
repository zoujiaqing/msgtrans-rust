pub mod api;
pub mod config;
pub mod pool;
pub mod expert_config;
pub mod client;
pub mod server;

// 重新导出核心API (使用api模块的实现)
pub use api::{
    Transport, TransportBuilder, ConnectionManager, ServerManager
};

// 重新导出配置和其他核心类型
pub use config::TransportConfig;

// 重新导出协议适配器
pub use crate::protocol::adapter::ProtocolAdapter;

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

// 重新导出分离式Builder和传输层
pub use client::{
    TransportClientBuilder, ClientTransport, ProtocolConnectionBuilder,
    ConnectionPoolConfig, RetryConfig, LoadBalancerConfig, CircuitBreakerConfig,
    ConnectionOptions, ConnectionPriority
};
pub use server::{
    TransportServerBuilder, ServerTransport, ProtocolServerBuilder,
    AcceptorConfig, BackpressureStrategy, RateLimiterConfig, ServerMiddleware,
    ServerOptions, LoggingMiddleware, AuthMiddleware
};
