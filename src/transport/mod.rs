/// 🚀 Phase 3: 传输层模块 - 渐进式高性能集成
/// 
/// ## 架构策略：平衡性能与兼容性
/// 
/// ### 当前集成状态 (Phase 3.2)
/// - 🚀 **后端高性能**：LockFree连接池 + 优化内存池（默认启用）
/// - 🔧 **前端兼容性**：传统Actor系统（保证现有代码正常工作）
/// - 📈 **渐进迁移**：OptimizedActor准备就绪，等待完全集成时机
/// 
/// ### 高性能组件架构
/// ```
/// ┌─────────────────────────────────────────────────────────────┐
/// │                   🚀 高性能传输层                            │
/// ├─────────────────┬───────────────────┬───────────────────────┤
/// │   前端API层      │    核心处理层      │     后端存储层         │
/// ├─────────────────┼───────────────────┼───────────────────────┤
/// │ ✅ Transport API │ 🔧 GenericActor   │ 🚀 LockFree连接池     │
/// │ ✅ 统一接口      │ 🔧 传统兼容        │ 🚀 优化内存池         │
/// │ ✅ 零配置        │ 📈 待升级          │ 🚀 详细监控           │
/// └─────────────────┴───────────────────┴───────────────────────┘
/// ```
/// 
/// ### 用户体验
/// - **零配置高性能**：`TransportBuilder::new().build()` 自动享受后端优化
/// - **完整向后兼容**：现有代码无需修改即可运行
/// - **透明性能提升**：内存分配和连接管理自动使用高性能实现
/// 
/// ### 未来演进路径
/// 1. **Phase 3.3**: 前端Actor层完全迁移到OptimizedActor
/// 2. **Phase 3.4**: 协议层集成FlumePoweredProtocolAdapter
/// 3. **Phase 4.0**: 端到端零拷贝数据路径

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
    LockFreeActorStats as ActorStats
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
    // 注意：原始 MemoryPool 已经删除，legacy 用户应该直接使用 OptimizedMemoryPool
    // 这里提供一个兼容别名
    pub use super::memory_pool_v2::{
        OptimizedMemoryPool as LegacyMemoryPool, 
        OptimizedMemoryStats as LegacyMemoryPoolStatus
    };
    
    // 旧的actor实现
    pub use crate::actor::{
        GenericActor as LegacyActor,
        ActorHandle as LegacyActorHandle,
        ActorManager as LegacyActorManager
    };
}
