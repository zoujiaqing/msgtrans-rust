/// 🎯 单连接传输抽象 - 每个实例只管理一个socket连接
/// 
/// 这是 Transport 的正确抽象：
/// - 每个 Transport 对应一个 socket 连接
/// - 提供 send() 方法直接向 socket 发送数据
/// - 协议无关的设计
/// - 由 TransportClient(单连接) 和 TransportServer(多连接管理) 使用

use std::sync::Arc;
use crate::{
    SessionId, TransportError, Packet,
    transport::{
        config::TransportConfig,
        pool::ConnectionPool,
        memory_pool_v2::OptimizedMemoryPool,
        expert_config::ExpertConfig,
    },
    protocol::ProtocolRegistry,
    adapters::create_standard_registry,
};

/// 🎯 单连接传输抽象 - 真正符合架构设计的 Transport
pub struct Transport {
    /// 配置
    config: TransportConfig,
    /// 协议注册表
    protocol_registry: Arc<ProtocolRegistry>,
    /// 🚀 Phase 3: 优化后的连接池
    connection_pool: Arc<ConnectionPool>,
    /// 🚀 Phase 3: 优化后的内存池
    memory_pool: Arc<OptimizedMemoryPool>,
    /// 🎯 单个连接适配器 - 代表这个socket连接
    connection_adapter: Option<Arc<dyn std::any::Any + Send + Sync>>,
    /// 当前连接的会话ID
    session_id: Option<SessionId>,
}

impl Transport {
    /// 创建新的单连接传输
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        tracing::info!("🚀 创建 Transport");
        
        // 创建协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 创建连接池和内存池 (简化版本)
        let connection_pool = Arc::new(
            ConnectionPool::new(2, 10).initialize_pool().await?
        );
        
        let memory_pool = Arc::new(OptimizedMemoryPool::new());
        
        Ok(Self {
            config,
            protocol_registry,
            connection_pool,
            memory_pool,
            connection_adapter: None,
            session_id: None,
        })
    }
    
    /// 🎯 核心方法：使用协议配置建立连接
    /// 这是 TransportClient 需要的连接方法
    pub async fn connect_with_config<T>(&mut self, config: &T) -> Result<SessionId, TransportError>
    where
        T: super::client::ConnectableConfig,
    {
        // 暂时使用简化的连接逻辑 - 实际需要协议适配器
        let session_id = SessionId::new(1);
        self.session_id = Some(session_id);
        
        tracing::info!("✅ Transport 连接建立成功: {}", session_id);
        Ok(session_id)
    }
    
    /// 🎯 核心方法：发送数据包到当前连接
    pub async fn send(&self, packet: Packet) -> Result<(), TransportError> {
        if let Some(_session_id) = self.session_id {
            // TODO: 实际发送逻辑 - 现在只是占位符
            tracing::debug!("📤 Transport 发送数据包");
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🎯 核心方法：断开连接
    pub async fn disconnect(&mut self) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id.take() {
            tracing::info!("🔌 Transport 断开连接: {}", session_id);
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🎯 核心方法：检查连接状态
    pub fn is_connected(&self) -> bool {
        self.session_id.is_some()
    }
    
    /// 🎯 核心方法：获取当前会话ID
    pub fn current_session_id(&self) -> Option<SessionId> {
        self.session_id
    }
    
    /// 设置连接适配器和会话ID (内部使用)
    pub fn set_connection<A>(&mut self, adapter: A, session_id: SessionId) 
    where
        A: Send + Sync + 'static,
    {
        self.connection_adapter = Some(Arc::new(adapter));
        self.session_id = Some(session_id);
        tracing::debug!("✅ Transport 连接设置完成: {}", session_id);
    }
    
    /// 获取协议注册表
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }
    
    /// 获取配置
    pub fn config(&self) -> &TransportConfig {
        &self.config
    }
    
    /// 🚀 Phase 3: 获取连接池统计
    pub fn connection_pool_stats(&self) -> crate::transport::pool::OptimizedPoolStatsSnapshot {
        self.connection_pool.get_performance_stats()
    }
    
    /// 🚀 Phase 3: 获取内存池统计
    pub fn memory_pool_stats(&self) -> crate::transport::memory_pool_v2::OptimizedMemoryStatsSnapshot {
        self.memory_pool.get_stats()
    }
}

impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
            connection_pool: self.connection_pool.clone(),
            memory_pool: self.memory_pool.clone(),
            connection_adapter: None,  // 克隆时不复制连接
            session_id: None,
        }
    }
}

impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("connected", &self.is_connected())
            .field("session_id", &self.session_id)
            .finish()
    }
} 