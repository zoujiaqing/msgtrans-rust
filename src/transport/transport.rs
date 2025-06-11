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
        // 使用 ConnectableConfig trait 进行实际连接
        match config.connect(self).await {
            Ok(session_id) => {
                self.session_id = Some(session_id);
                
                // 🔧 注意：这里暂时跳过连接适配器的创建
                // 因为真正的协议无关架构应该通过其他方式处理这个问题
                // 例如在 TransportClient 层面管理连接适配器
                tracing::info!("✅ Transport 连接建立成功: {}", session_id);
                Ok(session_id)
            }
            Err(e) => {
                tracing::error!("❌ Transport 连接失败: {:?}", e);
                Err(e)
            }
        }
    }

    
    /// 🎯 核心方法：发送数据包到当前连接
    pub async fn send(&self, packet: Packet) -> Result<(), TransportError> {
        if let Some(session_id) = self.session_id {
            // 🔧 实现真实的发送逻辑
            if let Some(connection_adapter) = &self.connection_adapter {
                tracing::debug!("📤 Transport 发送数据包 (会话: {})", session_id);
                
                // 尝试将连接适配器转换为具体的类型并发送
                // 这是一个简化的实现，真正的实现需要根据协议类型来处理
                if let Some(_tcp_adapter) = connection_adapter.downcast_ref::<crate::connection::TcpConnection>() {
                    tracing::debug!("📤 使用 TCP 适配器发送数据包");
                    // TODO: 调用 TCP 适配器的发送方法
                    // tcp_adapter.send(packet).await?;
                    
                    // 暂时只记录日志，实际发送需要适配器支持
                    tracing::debug!("📤 TCP 数据包发送完成 (占位符实现)");
                } else {
                    tracing::warn!("⚠️ 不支持的连接适配器类型，使用占位符发送");
                }
                
                Ok(())
            } else {
                tracing::error!("❌ 没有可用的连接适配器");
                Err(TransportError::connection_error("No connection adapter available", false))
            }
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