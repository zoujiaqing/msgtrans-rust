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
/// - **零配置高性能**：`TransportClientBuilder::new().build()` 和 `TransportServerBuilder::new().build()` 自动享受后端优化
/// - **完整向后兼容**：现有代码无需修改即可运行
/// - **透明性能提升**：内存分配和连接管理自动使用高性能实现
/// 
/// ### 未来演进路径
/// 1. **Phase 3.3**: 前端Actor层完全迁移到OptimizedActor
/// 2. **Phase 3.4**: 协议层集成FlumePoweredProtocolAdapter
/// 3. **Phase 4.0**: 端到端零拷贝数据路径

pub mod config;
pub mod pool;
pub mod expert_config;
pub mod client;
pub mod server;
pub mod transport;
pub mod transport_server;
pub mod connection_state;
pub mod request_manager;

// 🚀 Phase 3: 核心高性能组件 (默认启用)
pub mod lockfree;
pub mod lockfree_connection;
pub mod memory_pool;
pub mod protocol_adapter;
pub mod actor;

// 🚀 Phase 4: 架构清理完成 - 传统组件已完全移除
// OptimizedActor 已成为唯一的Actor实现

// 重新导出核心API - 使用新的架构
pub use transport::Transport;
pub use transport_server::TransportServer;
pub use client::{
    TransportClientBuilder, 
    TransportClient,
    ConnectionPoolConfig, RetryConfig, LoadBalancerConfig, CircuitBreakerConfig,
    ConnectionOptions, ConnectionPriority
};
pub use server::TransportServerBuilder;

// 重新导出配置
pub use config::TransportConfig;

// 重新导出协议适配器 trait
pub use crate::protocol::adapter::ProtocolAdapter as ProtocolAdapterTrait;

// 🚀 优化组件导出 (统一命名)
pub use memory_pool::{
    OptimizedMemoryPool as MemoryPool,
    OptimizedMemoryStats as MemoryStats, 
    OptimizedMemoryStatsSnapshot as MemoryStatsSnapshot,
    MemoryPoolEvent, BufferSize
};

pub use protocol_adapter::{
    FlumePoweredProtocolAdapter as ProtocolAdapter,
    LockFreeProtocolStats as ProtocolStats, 
    ProtocolStatsSnapshot,
    ProtocolEvent, 
    PerformanceMetrics,
    create_test_packet
};

pub use actor::{
    OptimizedActor as Actor,
    ActorManager, ActorCommand, ActorEvent, 
    LockFreeActorStats as ActorStats
};

// 连接池导出
pub use pool::{
    ConnectionPool, ExpansionStrategy, PoolDetailedStatus,
    OptimizedPoolStatsSnapshot
};

// 专家配置导出
pub use expert_config::{
    SmartPoolConfig, PerformanceConfig, ExpertConfig
};

// 客户端和服务端Builder导出
pub use server::{
    AcceptorConfig, BackpressureStrategy, RateLimiterConfig, ServerMiddleware,
    ServerOptions, LoggingMiddleware, AuthMiddleware
};

// LockFree核心组件导出
pub use lockfree::{
    LockFreeHashMap, LockFreeQueue, LockFreeCounter,
    LockFreeStats, QueueStats, CounterStats
};

// 无锁连接导出
pub use lockfree_connection::{
    LockFreeConnection, LockFreeConnectionStats, LockFreeConnectionCommand
};

// 连接状态管理导出
pub use connection_state::{
    ConnectionState, ConnectionStateManager
};

use bytes::Bytes;
use std::time::Duration;
use crate::packet::CompressionType;

/// 传输选项 - 用于自定义发送/请求行为
#[derive(Default, Clone, Debug)]
pub struct TransportOptions {
    /// 超时时间（仅对 request 有效）
    pub timeout: Option<Duration>,
    /// 压缩算法
    pub compression: Option<CompressionType>,
    /// 应用层业务类型 ID
    pub biz_type: Option<u8>,
    /// 扩展头内容（业务层自己编码）
    pub ext_header: Option<Bytes>,
    /// 消息 ID（可选，默认自动生成）
    pub message_id: Option<u32>,
}

impl TransportOptions {
    /// 创建新的传输选项
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置超时时间
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    /// 设置压缩算法
    pub fn with_compression(mut self, compression: CompressionType) -> Self {
        self.compression = Some(compression);
        self
    }
    
    /// 设置业务类型
    pub fn with_biz_type(mut self, biz_type: u8) -> Self {
        self.biz_type = Some(biz_type);
        self
    }
    
    /// 设置扩展头
    pub fn with_ext_header(mut self, ext_header: Bytes) -> Self {
        self.ext_header = Some(ext_header);
        self
    }
    
    /// 设置消息 ID
    pub fn with_message_id(mut self, message_id: u32) -> Self {
        self.message_id = Some(message_id);
        self
    }
    

}


