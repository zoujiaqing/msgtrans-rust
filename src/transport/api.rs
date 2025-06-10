/// 统一API接口层
/// 
/// 提供高级的、协议无关的传输API
/// 🚀 Phase 3: 默认使用优化后的高性能组件
/// 🚀 Phase 4: 简化架构 - 直接管理OptimizedActor
/// 移除了复杂的ActorHandle包装层，直接与OptimizedActor通信

use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::broadcast;
use crate::{
    SessionId, PacketId, TransportError, Packet, TransportEvent, EventStream,
    error::CloseReason,
    command::{ConnectionInfo, TransportCommand, TransportStats},
    protocol::{ProtocolAdapter, ProtocolConfig, ProtocolRegistry, Connection, ProtocolConnectionAdapter, adapter::ServerConfig},
    adapters::create_standard_registry,
    transport::{
        actor_v2::{ActorManager, OptimizedActor},
        lockfree_enhanced::LockFreeHashMap,
        pool::ConnectionPool,
        memory_pool_v2::{OptimizedMemoryPool, OptimizedMemoryStatsSnapshot, MemoryPoolEvent, BufferSize},
        expert_config::ExpertConfig,
    },
};
use futures::StreamExt;
use super::{
    config::TransportConfig,
};

/// 🔌 可连接配置 trait - 让每个协议自己处理连接逻辑
#[async_trait::async_trait]
pub trait ConnectableConfig: Send + Sync {
    /// 协议配置自己知道如何建立连接
    async fn connect(&self, transport: &Transport) -> Result<SessionId, TransportError>;
}

/// 协议信息
#[derive(Debug, Clone)]
pub struct ProtocolInfo {
    /// 协议名称
    pub name: String,
    /// 协议描述
    pub description: String,
    /// 协议特性
    pub features: Vec<String>,
    /// 默认端口
    pub default_port: Option<u16>,
}

/// 统一传输接口
/// 
/// 🚀 Phase 3: 默认集成高性能组件
/// 🚀 简化的会话管理器
pub struct Transport {
    /// 全局事件流
    #[allow(dead_code)]
    event_stream: EventStream,
    /// 会话ID生成器
    session_id_generator: Arc<AtomicU64>,
    /// 配置
    config: TransportConfig,
    /// 协议注册表
    protocol_registry: Arc<ProtocolRegistry>,
    /// 预配置的服务器
    configured_servers: Vec<Box<dyn crate::protocol::Server>>,
    /// 🚀 Phase 3: 优化后的连接池（默认启用无锁模式）
    connection_pool: Arc<ConnectionPool>,
    /// 🚀 Phase 3: 优化后的内存池
    memory_pool: Arc<OptimizedMemoryPool>,
    /// 🚀 简化的会话管理器
    session_manager: SimplifiedSessionManager,
}

impl Transport {
    /// 创建新的传输实例
    pub async fn new(config: TransportConfig) -> Result<Self, TransportError> {
        // 使用默认专家配置
        let expert_config = super::expert_config::ExpertConfig::default();
        Self::new_with_expert_config(config, expert_config).await
    }
    
    /// 使用专家配置创建传输实例
    pub async fn new_with_expert_config(
        config: TransportConfig, 
        expert_config: super::expert_config::ExpertConfig
    ) -> Result<Self, TransportError> {
        let session_manager = SimplifiedSessionManager::new();
        let event_stream = EventStream::new(session_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 🚀 Phase 3: 创建高性能组件（基于专家配置）
        let smart_pool = expert_config.smart_pool.unwrap_or_default();
        let performance = expert_config.performance.unwrap_or_default();
        
        let connection_pool = ConnectionPool::new(
            smart_pool.initial_size,
            smart_pool.max_size,
        ).initialize_pool().await?;
        let connection_pool = Arc::new(connection_pool);
        
        let memory_pool = Arc::new(OptimizedMemoryPool::new());
        
        tracing::info!("🚀 Transport 创建 (Expert Config):");
        tracing::info!("   ✅ 连接池 (初始: {}, 最大: {})", 
            smart_pool.initial_size,
            smart_pool.max_size);
        tracing::info!("   ✅ 优化内存池 (缓存: 1000)");
        tracing::info!("   ✅ 详细监控: {}", performance.enable_detailed_monitoring);
        
        Ok(Self {
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
            configured_servers: Vec::new(),
            connection_pool,
            memory_pool,
            session_manager,
        })
    }

    /// 🚀 Phase 4: 简化的连接添加 - 直接使用OptimizedActor
    pub async fn add_connection<A: ProtocolAdapter>(
        &self,
        adapter: A,
    ) -> Result<SessionId, TransportError> 
    where
        A: Send + 'static,
        A::Config: Send + 'static,
    {
        let session_id = self.generate_session_id();
        
        // 使用简化的会话管理器添加会话
        self.session_manager.add_session(session_id, adapter).await?;
        
        tracing::info!("✅ 成功添加 OptimizedActor 连接 (会话: {})", session_id);
        
        Ok(session_id)
    }
    
    /// 发送数据包到指定会话
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        message: Packet,
    ) -> Result<(), TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let command = TransportCommand::Send {
            session_id,
            packet: message,
            response_tx,
        };
        
        self.session_manager.send_to_session(session_id, command).await?;
        response_rx.await.map_err(|_| TransportError::connection_error("Response channel closed", false))?
    }
    
    /// 广播数据包到所有会话
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let sessions = self.session_manager.active_sessions();
        let mut errors = Vec::new();
        
        for session_id in sessions {
            if let Err(e) = self.send_to_session(session_id, packet.clone()).await {
                errors.push((session_id, e));
            }
        }
        
        if !errors.is_empty() {
            Err(TransportError::protocol_error("broadcast", format!("Broadcast failed to {} sessions", errors.len())))
        } else {
            Ok(())
        }
    }
    
    /// 🚀 Phase 3: 获取连接池统计
    pub fn connection_pool_stats(&self) -> super::pool::OptimizedPoolStatsSnapshot {
        self.connection_pool.get_performance_stats()
    }
    
    /// 🚀 Phase 3: 获取内存池统计
    pub fn memory_pool_stats(&self) -> super::memory_pool_v2::OptimizedMemoryStatsSnapshot {
        self.memory_pool.get_stats()
    }
    
    /// 🚀 Phase 3: 获取高性能组件引用
    pub fn connection_pool(&self) -> Arc<ConnectionPool> {
        self.connection_pool.clone()
    }
    
    pub fn memory_pool(&self) -> Arc<OptimizedMemoryPool> {
        self.memory_pool.clone()
    }
    
    /// 获取所有活跃会话
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        self.session_manager.active_sessions()
    }
    
    /// 获取会话连接信息  
    pub async fn session_info(&self, session_id: SessionId) -> Result<ConnectionInfo, TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let command = TransportCommand::GetConnectionInfo {
            session_id,
            response_tx,
        };
        
        self.session_manager.send_to_session(session_id, command).await?;
        response_rx.await.map_err(|_| TransportError::connection_error("Response channel closed", false))?
    }
    
    /// 获取传输统计信息
    pub async fn stats(&self) -> Result<HashMap<SessionId, TransportStats>, TransportError> {
        let sessions = self.session_manager.active_sessions();
        let mut stats = HashMap::new();
        
        for session_id in sessions {
            // 这里可以实现获取每个会话的统计信息
            // 暂时返回默认的统计信息
            let transport_stats = TransportStats::new();
            stats.insert(session_id, transport_stats);
        }
        
        Ok(stats)
    }
    
    /// 获取事件流
    pub fn events(&self) -> EventStream {
        let receiver = self.session_manager.global_events();
        EventStream::new(receiver)
    }
    
    /// 获取特定会话的事件流
    pub fn session_events(&self, session_id: SessionId) -> EventStream {
        let receiver = self.session_manager.global_events();
        EventStream::with_session_filter(receiver, session_id)
    }
    
    /// 🔌 统一连接方法 - 真正可扩展的设计
    pub async fn connect<T>(&self, config: T) -> Result<SessionId, TransportError> 
    where 
        T: ConnectableConfig,
    {
        config.connect(self).await
    }
    
    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        SessionId::new(self.session_id_generator.fetch_add(1, Ordering::SeqCst))
    }
    
    /// 🚀 启动所有预配置的服务器 (消费 self 来获得所有权)
    pub async fn serve(mut self) -> Result<(), TransportError> {
        if self.configured_servers.is_empty() {
            tracing::warn!("没有配置任何协议服务器，启动空的传输实例");
            return Ok(());
        }
        
        tracing::info!("🌟 启动 {} 个预配置的协议服务器", self.configured_servers.len());
        
        // 移动所有服务器并为每个启动接受循环
        let servers = std::mem::take(&mut self.configured_servers);
        let mut server_handles = Vec::new();
        
        for (index, mut server) in servers.into_iter().enumerate() {
            let transport = self.clone();
            let server_index = index;
            
            tracing::info!("📡 启动第 {} 个协议服务器的接受循环", server_index + 1);
            
            // 启动每个服务器的接受循环
            let handle = tokio::spawn(async move {
                tracing::info!("🎯 协议服务器 {} 开始接受连接", server_index + 1);
                
                loop {
                    match server.accept().await {
                        Ok(mut connection) => {
                            let conn_session_id = transport.generate_session_id();
                            tracing::debug!("🔗 服务器 {} 接受到新连接 (会话ID: {})", server_index + 1, conn_session_id);
                            
                            connection.set_session_id(conn_session_id);
                            
                            let adapter = ProtocolConnectionAdapter::new(connection);
                            match transport.add_connection(adapter).await {
                                Ok(_) => {
                                    tracing::debug!("✅ 成功添加连接到传输层 (会话ID: {})", conn_session_id);
                                }
                                Err(e) => {
                                    tracing::error!("❌ 添加连接失败 (会话ID: {}): {:?}", conn_session_id, e);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!("⚠️ 协议服务器 {} 接受连接时出错: {:?}", server_index + 1, e);
                            break;
                        }
                    }
                }
                
                tracing::warn!("🛑 协议服务器 {} 接受循环已退出", server_index + 1);
            });
            
            server_handles.push(handle);
        }
        
        tracing::info!("✅ 所有 {} 个协议服务器启动完成，等待连接...", server_handles.len());
        
        // 🔧 修复：不再创建内置事件处理循环，让用户自己处理事件
        // 这样用户在调用serve()之前创建的事件流就能正常工作
        
        // 等待所有任务完成（实际上是永远运行）
        futures::future::join_all(server_handles).await;
        
        tracing::info!("🏁 传输服务已停止");
        Ok(())
    }
    
    /// 获取协议注册表的引用
    pub fn protocol_registry(&self) -> &ProtocolRegistry {
        &self.protocol_registry
    }
    
    /// 列出所有已注册的协议
    pub async fn list_protocols(&self) -> Vec<String> {
        self.protocol_registry.list_protocols().await
    }
    
    /// 添加协议连接到Actor管理
    async fn add_protocol_connection(&self, mut connection: Box<dyn Connection>) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        connection.set_session_id(session_id);
        
        // 创建一个适配器包装器来兼容现有的add_connection方法
        let adapter = ProtocolConnectionAdapter::new(connection);
        self.add_connection(adapter).await
    }
    
    /// 添加协议服务器并开始接受连接
    async fn add_protocol_server(&self, mut server: Box<dyn crate::protocol::Server>) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        let transport = self.clone();
        
        tracing::debug!("启动协议服务器接受循环 (服务器会话ID: {})", session_id);
        
        // 启动服务器接受循环
        tokio::spawn(async move {
            tracing::debug!("协议服务器接受循环已启动，等待客户端连接...");
            
            loop {
                tracing::debug!("等待新的客户端连接...");
                
                match server.accept().await {
                    Ok(mut connection) => {
                        let conn_session_id = transport.generate_session_id();
                        tracing::info!("协议服务器接受到新连接 (连接会话ID: {})", conn_session_id);
                        
                        connection.set_session_id(conn_session_id);
                        
                        let adapter = ProtocolConnectionAdapter::new(connection);
                        match transport.add_connection(adapter).await {
                            Ok(_) => {
                                tracing::info!("成功添加协议连接到传输层 (会话ID: {})", conn_session_id);
                            }
                            Err(e) => {
                                tracing::error!("添加协议连接失败 (会话ID: {}): {:?}", conn_session_id, e);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("协议服务器接受连接时出错: {:?}", e);
                        break;
                    }
                }
            }
            
            tracing::warn!("协议服务器接受循环已退出");
        });
        
        Ok(session_id)
    }
    
    /// 创建客户端连接 - 公共方法
    pub async fn create_client_connection(&self, protocol_config: &dyn crate::protocol::adapter::DynProtocolConfig) -> Result<SessionId, TransportError> {
        // 使用配置的ConnectableConfig trait来建立连接
        let protocol_name = protocol_config.protocol_name();
        tracing::debug!("🔌 创建客户端连接，协议: {}", protocol_name);
        
        match protocol_name {
            "tcp" => {
                if let Some(tcp_config) = protocol_config.as_any().downcast_ref::<crate::protocol::TcpClientConfig>() {
                    tcp_config.connect(self).await
                } else {
                    Err(TransportError::config_error("protocol", "Invalid TCP client config"))
                }
            }
            "websocket" => {
                if let Some(ws_config) = protocol_config.as_any().downcast_ref::<crate::protocol::WebSocketClientConfig>() {
                    ws_config.connect(self).await
                } else {
                    Err(TransportError::config_error("protocol", "Invalid WebSocket client config"))
                }
            }
            "quic" => {
                if let Some(quic_config) = protocol_config.as_any().downcast_ref::<crate::protocol::QuicClientConfig>() {
                    quic_config.connect(self).await
                } else {
                    Err(TransportError::config_error("protocol", "Invalid QUIC client config"))
                }
            }
            _ => {
                Err(TransportError::config_error("protocol", format!("Unsupported protocol: {}", protocol_name)))
            }
        }
    }

    /// 获取支持的协议列表
    pub fn supported_protocols() -> Vec<&'static str> {
        vec!["tcp", "websocket", "quic"]
    }
    
    /// 检查是否支持指定协议
    pub fn supports_protocol(protocol: &str) -> bool {
        Self::supported_protocols().contains(&protocol)
    }
    
    /// 获取协议的详细信息
    pub fn protocol_info(protocol: &str) -> Option<ProtocolInfo> {
        match protocol {
            "tcp" => Some(ProtocolInfo {
                name: "tcp".to_string(),
                description: "Transmission Control Protocol - reliable, ordered, connection-oriented".to_string(),
                features: vec!["reliable".to_string(), "ordered".to_string(), "connection-oriented".to_string()],
                default_port: Some(8080),
            }),
            "websocket" => Some(ProtocolInfo {
                name: "websocket".to_string(),
                description: "WebSocket Protocol - full-duplex communication over HTTP".to_string(),
                features: vec!["full-duplex".to_string(), "http-upgrade".to_string(), "frame-based".to_string()],
                default_port: Some(8080),
            }),
            "quic" => Some(ProtocolInfo {
                name: "quic".to_string(),
                description: "QUIC Protocol - modern transport with built-in encryption".to_string(),
                features: vec!["encrypted".to_string(), "multiplexed".to_string(), "low-latency".to_string()],
                default_port: Some(4433),
            }),
            _ => None,
        }
    }

    /// 关闭指定会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        self.session_manager.close_session(session_id).await?;
        tracing::debug!("👋 会话 {} 已关闭", session_id);
        Ok(())
    }
}

/// 传输构建器
/// 
/// 用于创建配置好的传输实例
pub struct TransportBuilder {
    config: TransportConfig,
    expert_config: super::expert_config::ExpertConfig,
    /// 协议配置存储
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
}

impl TransportBuilder {
    /// 创建新的传输构建器
    pub fn new() -> Self {
        Self {
            config: TransportConfig::default(),
            expert_config: super::expert_config::ExpertConfig::default(),
            protocol_configs: std::collections::HashMap::new(),
        }
    }
    
    /// 🌟 统一协议配置接口 - 支持所有协议
    pub fn with_protocol_config<T: crate::protocol::adapter::DynProtocolConfig>(mut self, config: T) -> Self {
        let protocol_name = config.protocol_name().to_string();
        self.protocol_configs.insert(protocol_name, Box::new(config));
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: TransportConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 设置智能连接池配置
    pub fn with_smart_pool_config(mut self, config: super::expert_config::SmartPoolConfig) -> Self {
        self.expert_config.smart_pool = Some(config);
        self
    }
    
    /// 设置性能监控配置
    pub fn with_performance_config(mut self, config: super::expert_config::PerformanceConfig) -> Self {
        self.expert_config.performance = Some(config);
        self
    }
    
    /// 启用高性能预设配置
    pub fn high_performance(mut self) -> Self {
        self.expert_config.smart_pool = Some(super::expert_config::SmartPoolConfig::high_performance());
        self.expert_config.performance = Some(super::expert_config::PerformanceConfig::production());
        self
    }
    
    /// 构建传输实例 - 预先创建所有配置的服务器
    pub async fn build(self) -> Result<Transport, TransportError> {
        // 验证基础配置
        self.config.validate()
            .map_err(|e| TransportError::config_error("protocol", format!("Invalid config: {:?}", e)))?;
        
        // 验证专家配置
        self.expert_config.validate()
            .map_err(|e| TransportError::config_error("expert", format!("Invalid expert config: {:?}", e)))?;
        
        // 预先创建所有配置的服务器
        let mut configured_servers: Vec<Box<dyn crate::protocol::Server>> = Vec::new();
        
        tracing::info!("🔧 构建传输实例，处理 {} 个协议配置", self.protocol_configs.len());
        
        for (protocol_name, config) in &self.protocol_configs {
            tracing::info!("  🌐 构建 {} 协议服务器", protocol_name);
            
            // 验证协议配置
            config.validate_dyn()
                .map_err(|e| TransportError::config_error("protocol", format!("Invalid {} config: {:?}", protocol_name, e)))?;
            
            // 根据协议类型创建服务器
            match protocol_name.as_str() {
                "tcp" => {
                    if let Some(tcp_config) = config.as_any().downcast_ref::<crate::protocol::TcpServerConfig>() {
                        let server = tcp_config.build_server().await
                            .map_err(|e| TransportError::protocol_error("tcp", format!("Failed to create TCP server: {:?}", e)))?;
                        configured_servers.push(Box::new(server));
                        tracing::info!("    ✅ TCP 服务器创建成功 ({})", tcp_config.bind_address);
                    }
                }
                #[cfg(feature = "websocket")]
                "websocket" => {
                    if let Some(ws_config) = config.as_any().downcast_ref::<crate::protocol::WebSocketServerConfig>() {
                        let server = ws_config.build_server().await
                            .map_err(|e| TransportError::protocol_error("websocket", format!("Failed to create WebSocket server: {:?}", e)))?;
                        configured_servers.push(Box::new(server));
                        tracing::info!("    ✅ WebSocket 服务器创建成功 ({})", ws_config.bind_address);
                    }
                }
                #[cfg(feature = "quic")]
                "quic" => {
                    if let Some(quic_config) = config.as_any().downcast_ref::<crate::protocol::QuicServerConfig>() {
                        let server = quic_config.build_server().await
                            .map_err(|e| TransportError::protocol_error("quic", format!("Failed to create QUIC server: {:?}", e)))?;
                        configured_servers.push(Box::new(server));
                        tracing::info!("    ✅ QUIC 服务器创建成功 ({})", quic_config.bind_address);
                    }
                }
                _ => {
                    tracing::warn!("    ⚠️ 未知协议类型: {}", protocol_name);
                }
            }
        }
        
        // 如果启用了专家配置，记录日志
        if self.expert_config.has_expert_config() {
            tracing::info!("🚀 启用专家配置模式");
            
            if let Some(ref pool_config) = self.expert_config.smart_pool {
                tracing::info!("  📊 智能连接池: {}→{} (阈值: {:.0}%→{:.0}%)", 
                    pool_config.initial_size, 
                    pool_config.max_size,
                    pool_config.expansion_threshold * 100.0,
                    pool_config.shrink_threshold * 100.0
                );
            }
            
            if let Some(ref perf_config) = self.expert_config.performance {
                tracing::info!("  📈 性能监控: {}ms采样, {}条历史记录", 
                    perf_config.sampling_interval.as_millis(),
                    perf_config.metrics_history_size
                );
            }
        }
        
        // 创建带预配置服务器的Transport实例
        Self::new_transport_with_servers(self.config, self.expert_config, configured_servers).await
    }
    
    /// 内部方法：创建带预配置服务器的Transport实例
    async fn new_transport_with_servers(
        config: TransportConfig,
        expert_config: super::expert_config::ExpertConfig,
        configured_servers: Vec<Box<dyn crate::protocol::Server>>,
    ) -> Result<Transport, TransportError> {
        let session_manager = SimplifiedSessionManager::new();
        let event_stream = EventStream::new(session_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 🚀 Phase 3: 创建高性能组件（基于专家配置）
        let smart_pool = expert_config.smart_pool.unwrap_or_default();
        let performance = expert_config.performance.unwrap_or_default();
        
        let connection_pool = ConnectionPool::new(
            smart_pool.initial_size,
            smart_pool.max_size,
        ).initialize_pool().await?;
        let connection_pool = Arc::new(connection_pool);
        
        let memory_pool = Arc::new(OptimizedMemoryPool::new());
        
        tracing::info!("🚀 Transport 创建 (Expert Config):");
        tracing::info!("   ✅ 连接池 (初始: {}, 最大: {})", 
            smart_pool.initial_size,
            smart_pool.max_size);
        tracing::info!("   ✅ 优化内存池 (缓存: 1000)");
        tracing::info!("   ✅ 详细监控: {}", performance.enable_detailed_monitoring);
        
        Ok(Transport {
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
            configured_servers,
            connection_pool,
            memory_pool,
            session_manager,
        })
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 连接管理器
/// 
/// 提供连接的高级管理功能
pub struct ConnectionManager {
    transport: Transport,
}

impl ConnectionManager {
    /// 创建新的连接管理器
    pub fn new(transport: Transport) -> Self {
        Self { transport }
    }
    
    /// 创建TCP连接
    pub async fn create_tcp_connection(
        &self,
        addr: std::net::SocketAddr,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::tcp::TcpClientBuilder;
        use crate::protocol::TcpClientConfig;
        
        let config = TcpClientConfig::default();
        let adapter = TcpClientBuilder::new()
            .target_address(addr)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("TCP connection failed: {:?}", e), true))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 创建WebSocket连接
    #[cfg(feature = "websocket")]
    pub async fn create_websocket_connection(
        &self,
        url: &str,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::websocket::{WebSocketClientBuilder};
        use crate::protocol::WebSocketClientConfig;
        
        let config = WebSocketClientConfig::default();
        let adapter = WebSocketClientBuilder::new()
            .target_url(url)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("WebSocket connection failed: {:?}", e), true))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 创建QUIC连接
    #[cfg(feature = "quic")]
    pub async fn create_quic_connection(
        &self,
        addr: std::net::SocketAddr,
    ) -> Result<SessionId, TransportError> {
        use crate::adapters::quic::{QuicClientBuilder};
        use crate::protocol::QuicClientConfig;
        
        let config = QuicClientConfig::default();
        let adapter = QuicClientBuilder::new()
            .target_address(addr)
            .config(config)
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("QUIC connection failed: {:?}", e), true))?;
        
        self.transport.add_connection(adapter).await
    }
    
    /// 获取内部传输实例的引用
    pub fn transport(&self) -> &Transport {
        &self.transport
    }
}

/// 服务器管理器
/// 
/// 管理多协议服务器
pub struct ServerManager {
    transport: Transport,
    servers: Arc<tokio::sync::Mutex<HashMap<String, ServerHandle>>>,
}

/// 服务器句柄
pub enum ServerHandle {
    WebSocket(crate::adapters::websocket::WebSocketServer<crate::protocol::WebSocketServerConfig>),
    Tcp(crate::adapters::factories::TcpServerWrapper),
    Quic(crate::adapters::quic::QuicServer),
}

impl ServerManager {
    /// 创建新的服务器管理器
    pub fn new(transport: Transport) -> Self {
        Self {
            transport,
            servers: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }
    
    /// 启动TCP服务器
    pub async fn start_tcp_server(
        &self,
        name: String,
        addr: std::net::SocketAddr,
    ) -> Result<(), TransportError> {
        use crate::adapters::tcp::TcpServerBuilder;
        use crate::protocol::TcpServerConfig;
        
        let config = TcpServerConfig::default();
        let server = TcpServerBuilder::new()
            .bind_address(addr)
            .config(config.clone())
            .build()
            .await
            .map_err(|e| TransportError::config_error("server", format!("Failed to start TCP server: {:?}", e)))?;
        
        // 启动接受循环
        let transport = self.transport.clone();
        let servers = self.servers.clone();
        let name_for_task = name.clone();
        
        // 先存储一个占位符，等spawn完成后再更新
        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.clone(), ServerHandle::Tcp(crate::adapters::factories::TcpServerWrapper::new(server)));
        }
        
        // 重新创建server用于spawn（临时解决方案）
        let server_for_spawn = TcpServerBuilder::new()
            .bind_address(addr)
            .config(config)
            .build()
            .await
            .map_err(|e| TransportError::config_error("server", format!("Failed to start TCP server: {:?}", e)))?;
        
        tokio::spawn(async move {
            let mut server = server_for_spawn;
            loop {
                match server.accept().await {
                    Ok(adapter) => {
                        if let Err(e) = transport.add_connection(adapter).await {
                            tracing::error!("Failed to add TCP connection: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("TCP server accept error: {:?}", e);
                        break;
                    }
                }
            }
            
            // 从服务器列表中移除
            servers.lock().await.remove(&name_for_task);
        });
        
        Ok(())
    }
    
    /// 启动WebSocket服务器
    #[cfg(feature = "websocket")]
    pub async fn start_websocket_server(
        &self,
        name: String,
        addr: std::net::SocketAddr,
    ) -> Result<(), TransportError> {
        use crate::adapters::websocket::WebSocketServerBuilder;
        use crate::protocol::WebSocketServerConfig;
        
        let config = WebSocketServerConfig::default();
        let server = WebSocketServerBuilder::new()
            .bind_address(addr)
            .config(config.clone())
            .build()
            .await
            .map_err(|e| TransportError::config_error("server", format!("Failed to start WebSocket server: {:?}", e)))?;
        
        // 启动接受循环
        let transport = self.transport.clone();
        let servers = self.servers.clone();
        let name_for_task = name.clone();
        
        // 先存储服务器句柄
        {
            let mut servers = self.servers.lock().await;
            servers.insert(name.clone(), ServerHandle::WebSocket(server));
        }
        
        // 重新创建server用于spawn
        let server_for_spawn = WebSocketServerBuilder::new()
            .bind_address(addr)
            .config(config)
            .build()
            .await
            .map_err(|e| TransportError::config_error("server", format!("Failed to start WebSocket server: {:?}", e)))?;
        
        tokio::spawn(async move {
            let mut server = server_for_spawn;
            loop {
                match server.accept().await {
                    Ok(adapter) => {
                        if let Err(e) = transport.add_connection(adapter).await {
                            tracing::error!("Failed to add WebSocket connection: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("WebSocket server accept error: {:?}", e);
                        break;
                    }
                }
            }
            
            // 从服务器列表中移除
            servers.lock().await.remove(&name_for_task);
        });
        
        Ok(())
    }
    
    /// 获取内部传输实例的引用
    pub fn transport(&self) -> &Transport {
        &self.transport
    }
}

// 为Transport实现Clone，使其可以在多个地方使用
impl Clone for Transport {
    fn clone(&self) -> Self {
        Self {
            event_stream: EventStream::new(self.session_manager.global_events()),
            session_id_generator: self.session_id_generator.clone(),
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
            configured_servers: Vec::new(), // 克隆时不复制服务器
            connection_pool: self.connection_pool.clone(),
            memory_pool: self.memory_pool.clone(),
            session_manager: self.session_manager.clone(), // 🔧 修复：共享同一个会话管理器
        }
    }
}

/// 🚀 Phase 1: 手动实现 Debug trait
impl std::fmt::Debug for Transport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transport")
            .field("session_count", &self.session_id_generator.load(std::sync::atomic::Ordering::Relaxed))
            .field("config", &self.config)
            .field("server_count", &self.configured_servers.len())
            .finish()
    }
}

/// 🚀 简化的会话管理器 - 重新设计避免Clone问题
pub struct SimplifiedSessionManager {
    /// 会话命令发送器映射：SessionId -> 命令发送器
    sessions: Arc<LockFreeHashMap<SessionId, mpsc::Sender<TransportCommand>>>,
    /// 会话状态映射：SessionId -> 会话状态
    session_states: Arc<LockFreeHashMap<SessionId, SessionState>>,
    /// 全局事件发送器
    global_event_sender: broadcast::Sender<TransportEvent>,
    /// 🔧 修复：保持活跃的接收器，防止广播频道关闭
    #[allow(dead_code)]
    _keep_alive_receiver: broadcast::Receiver<TransportEvent>,
}

/// 简化的会话状态
#[derive(Debug, Clone)]
pub struct SessionState {
    pub session_id: SessionId,
    pub created_at: std::time::Instant,
    pub last_activity: std::time::Instant,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SessionStatus {
    Active,
    Closing,
    Closed,
}

impl SimplifiedSessionManager {
    /// 创建新的会话管理器
    pub fn new() -> Self {
        let (global_event_sender, keep_alive_receiver) = broadcast::channel(10000);
        
        Self {
            sessions: Arc::new(LockFreeHashMap::new()),
            session_states: Arc::new(LockFreeHashMap::new()),
            global_event_sender,
            _keep_alive_receiver: keep_alive_receiver, // 🔧 修复：保持接收器活跃
        }
    }
    
    /// 添加会话
    pub async fn add_session<A: ProtocolAdapter>(
        &self,
        session_id: SessionId,
        adapter: A,
    ) -> Result<(), TransportError>
    where
        A: Send + 'static,
        A::Config: Send + 'static,
    {
        // 创建OptimizedActor
        let (optimized_actor, _event_receiver, _data_sender, command_sender) = 
            OptimizedActor::new_with_real_adapter(
                session_id,
                adapter,
                32,
                self.global_event_sender.clone(),
            );
        
        // 存储命令发送器
        if let Err(_) = self.sessions.insert(session_id, command_sender) {
            return Err(TransportError::connection_error("Failed to add session", false));
        }
        
        // 存储会话状态
        let session_state = SessionState {
            session_id,
            created_at: std::time::Instant::now(),
            last_activity: std::time::Instant::now(),
            status: SessionStatus::Active,
        };
        
        if let Err(_) = self.session_states.insert(session_id, session_state) {
            return Err(TransportError::connection_error("Failed to add session state", false));
        }
        
        // 启动OptimizedActor任务 - 无需存储JoinHandle
        let sessions = self.sessions.clone();
        let session_states = self.session_states.clone();
        tokio::spawn(async move {
            tracing::info!("🚀 启动 OptimizedActor (会话: {})", session_id);
            
            // 运行OptimizedActor
            if let Err(e) = optimized_actor.run_dual_pipeline().await {
                tracing::error!("OptimizedActor {} failed: {:?}", session_id, e);
            }
            
            // 清理会话
            let _ = sessions.remove(&session_id);
            let _ = session_states.remove(&session_id);
            
            tracing::info!("🛑 OptimizedActor 已退出 (会话: {})", session_id);
        });
        
        tracing::info!("✅ 成功添加会话 (会话: {})", session_id);
        Ok(())
    }
    
    /// 发送命令到会话
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        command: TransportCommand,
    ) -> Result<(), TransportError> {
        if let Some(sender) = self.sessions.get(&session_id) {
            sender.send(command).await.map_err(|_| {
                TransportError::connection_error("Session channel closed", false)
            })
        } else {
            Err(TransportError::connection_error("Session not found", false))
        }
    }
    
    /// 获取所有活跃会话
    pub fn active_sessions(&self) -> Vec<SessionId> {
        let mut sessions = Vec::new();
        self.sessions.for_each(|session_id, _| {
            sessions.push(*session_id);
        });
        sessions
    }
    
    /// 关闭会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        // 更新会话状态
        if let Some(mut state) = self.session_states.get(&session_id) {
            state.status = SessionStatus::Closing;
            let _ = self.session_states.insert(session_id, state);
        }
        
        // 发送关闭命令
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        let close_command = TransportCommand::ForceDisconnect {
            session_id,
            reason: "User requested".to_string(),
            response_tx,
        };
        
        if let Some(sender) = self.sessions.get(&session_id) {
            let _ = sender.send(close_command).await;
            let _ = response_rx.await;
        }
        
        // 移除会话
        let _ = self.sessions.remove(&session_id);
        let _ = self.session_states.remove(&session_id);
        
        Ok(())
    }
    
    /// 获取全局事件流
    pub fn global_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.global_event_sender.subscribe()
    }
    
    /// 会话数量
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
    
    /// 获取会话状态
    pub fn get_session_state(&self, session_id: SessionId) -> Option<SessionState> {
        self.session_states.get(&session_id)
    }
}

impl Clone for SimplifiedSessionManager {
    fn clone(&self) -> Self {
        // 🔧 修复：真正的克隆，共享所有组件
        // 注意：broadcast::Receiver无法clone，所以每次克隆时创建新的订阅
        Self {
            sessions: self.sessions.clone(),
            session_states: self.session_states.clone(),
            global_event_sender: self.global_event_sender.clone(),
            _keep_alive_receiver: self.global_event_sender.subscribe(),
        }
    }
} 