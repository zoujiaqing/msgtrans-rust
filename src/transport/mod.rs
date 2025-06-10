pub mod api;
pub mod config;
pub mod pool;
pub mod expert_config;
pub mod client;
pub mod server;

// 🚀 Phase 3: 核心高性能组件 (默认启用)
pub mod lockfree_enhanced;
pub mod memory_pool_v2;
pub mod protocol_adapter_v2;
pub mod actor_v2;

// 重新导出核心API (使用api模块的实现)
pub use api::{
    Transport, TransportBuilder, ConnectionManager, ServerManager
};

// 重新导出配置和其他核心类型
pub use config::TransportConfig;

// 重新导出协议适配器 trait (保持原有的trait接口)
pub use crate::protocol::adapter::ProtocolAdapter as ProtocolAdapterTrait;

// 🚀 Phase 3: 默认导出优化组件 (替代旧组件)
pub use memory_pool_v2::{
    OptimizedMemoryPool as MemoryPool,
    OptimizedMemoryStats as MemoryStats, 
    OptimizedMemoryStatsSnapshot as MemoryStatsSnapshot,
    MemoryPoolEvent, BufferSize
};

pub use protocol_adapter_v2::{
    FlumePoweredProtocolAdapter as ProtocolAdapter,
    LockFreeProtocolStats as ProtocolStats, 
    ProtocolStatsSnapshot,
    ProtocolEvent, 
    PerformanceMetrics as ProtocolPerformanceMetrics,
    create_test_packet
};

pub use actor_v2::{
    OptimizedActor as Actor,
    ActorManager, ActorCommand, ActorEvent, 
    ActorStats, LockFreeActorStats
};

// 🚀 Phase 3: 智能连接池 (已经是优化版本)
pub use pool::{
    ConnectionPool, ExpansionStrategy, PoolDetailedStatus,
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
    TransportServerBuilder, ServerTransport,
    AcceptorConfig, BackpressureStrategy, RateLimiterConfig, ServerMiddleware,
    ServerOptions, LoggingMiddleware, AuthMiddleware
};

// 🚀 Phase 3: LockFree核心组件
pub use lockfree_enhanced::{
    LockFreeHashMap, LockFreeQueue, LockFreeCounter,
    LockFreeStats, QueueStats, CounterStats
};

// 📦 Legacy 组件导出 (保持向后兼容)
pub mod legacy {
    pub use super::pool::{
        MemoryPool as LegacyMemoryPool, 
        MemoryPoolStatus as LegacyMemoryPoolStatus
    };
    
    // 旧的actor实现
    pub use crate::actor::{
        GenericActor as LegacyActor,
        ActorHandle as LegacyActorHandle,
        ActorManager as LegacyActorManager
    };
}
