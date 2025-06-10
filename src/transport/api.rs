/// 统一API接口层
/// 
/// 提供高级的、协议无关的传输API
/// 🚀 Phase 3: 默认使用优化后的高性能组件

use tokio::sync::mpsc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::collections::HashMap;
use crate::{
    SessionId,
    command::{TransportStats, ConnectionInfo},
    error::TransportError,
    actor::{ActorHandle, ActorManager},
    protocol::{ProtocolAdapter, ProtocolConfig, ProtocolRegistry, Connection, ProtocolConnectionAdapter, adapter::ServerConfig},
    stream::EventStream,
    packet::Packet,
    adapters::create_standard_registry,
    Event,
};
use futures::StreamExt;
use super::config::TransportConfig;

// 🚀 Phase 3: 默认使用优化组件
use super::{
    memory_pool_v2::OptimizedMemoryPool,
    ConnectionPool,
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
pub struct Transport {
    /// 🚀 优化后的Actor管理器
    actor_manager: Arc<ActorManager>,
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
        let actor_manager = Arc::new(ActorManager::new());
        let event_stream = EventStream::new(actor_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 🚀 Phase 3: 创建优化后的高性能组件
        let smart_pool = expert_config.smart_pool.unwrap_or_default();
        let performance = expert_config.performance.unwrap_or_default();
        
        let connection_pool = Arc::new(
            ConnectionPool::new(
                smart_pool.initial_size,
                smart_pool.max_size
            ).initialize_pool().await?
        );
        
        let memory_pool = Arc::new(
            OptimizedMemoryPool::new()
                .with_preallocation(
                    1000,  // 默认缓存大小
                    500,   
                    250
                )
        );
        
        tracing::info!("🚀 Transport 创建成功，默认启用高性能组件:");
        tracing::info!("   ✅ LockFree 连接池 (初始: {}, 最大: {})", 
                      smart_pool.initial_size,
                      smart_pool.max_size);
        tracing::info!("   ✅ 优化内存池 (缓存: 1000)");
        tracing::info!("   ✅ 详细监控: {}", performance.enable_detailed_monitoring);
        
        Ok(Self {
            actor_manager,
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
            configured_servers: Vec::new(),
            connection_pool,
            memory_pool,
        })
    }

    /// ✅ 使用外部 ActorManager 创建传输实例 (用于ServerTransport中的连接)
    pub async fn new_with_shared_actor_manager(
        config: TransportConfig,
        shared_actor_manager: Arc<ActorManager>,
    ) -> Result<Self, TransportError> {
        let event_stream = EventStream::new(shared_actor_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 🚀 Phase 3: 默认高性能组件
        let expert_config = super::expert_config::ExpertConfig::default();
        let smart_pool = expert_config.smart_pool.unwrap_or_default();
        
        let connection_pool = Arc::new(
            ConnectionPool::new(
                smart_pool.initial_size,
                smart_pool.max_size
            ).initialize_pool().await?
        );
        
        let memory_pool = Arc::new(OptimizedMemoryPool::new());
        
        tracing::debug!("✅ 使用共享ActorManager创建Transport实例（默认高性能组件）");
        
        Ok(Self {
            actor_manager: shared_actor_manager,
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
            configured_servers: Vec::new(),
            connection_pool,
            memory_pool,
        })
    }
    
    /// 🚀 Phase 3: 添加连接时默认使用优化组件
    pub async fn add_connection<A: ProtocolAdapter>(
        &self,
        adapter: A,
    ) -> Result<SessionId, TransportError> {
        let session_id = self.generate_session_id();
        
        // 🔧 暂时使用传统 GenericActor 确保兼容性
        // TODO: 后续完善 OptimizedActor 与 ActorHandle 的集成
        
        // 创建Actor的命令通道
        let (command_tx, command_rx) = mpsc::channel(1024);
        
        // 使用全局事件发送器
        let global_event_tx = self.actor_manager.global_event_tx.clone();
        let global_event_rx = self.actor_manager.global_events();
        
        // 创建传统Actor（但使用优化的内存池和连接池）
        let actor = crate::actor::GenericActor::new(
            adapter,
            session_id,
            command_rx,
            global_event_tx,
            A::Config::default_config(),
        );
        
        // 创建Actor句柄
        let handle = crate::actor::ActorHandle::new(
            command_tx,
            global_event_rx,
            session_id,
            Arc::new(tokio::sync::Mutex::new(0)),
        );
        
        // 添加到管理器
        self.actor_manager.add_actor(session_id, handle).await;
        
        // 启动Actor
        let actor_manager = self.actor_manager.clone();
        let session_id_for_cleanup = session_id;
        tokio::spawn(async move {
            if let Err(e) = actor.run().await {
                tracing::error!("Actor {} failed: {:?}", session_id_for_cleanup, e);
            }
            
            // 清理Actor
            actor_manager.remove_actor(&session_id_for_cleanup).await;
        });
        
        tracing::info!("✅ 会话 {} 已创建，使用高性能后端组件", session_id);
        
        Ok(session_id)
    }
    
    /// 发送数据包到指定会话
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        message: Packet,
    ) -> Result<(), TransportError> {
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.send_packet(message).await
        } else {
            Err(TransportError::connection_error("Session not found", false))
        }
    }
    
    /// 广播数据包到所有会话
    pub async fn broadcast(&self, packet: Packet) -> Result<(), TransportError> {
        let sessions = self.actor_manager.active_sessions().await;
        let mut errors = Vec::new();
        
        for session_id in sessions {
            if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
                if let Err(e) = handle.send_packet(packet.clone()).await {
                    errors.push((session_id, e));
                }
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
        self.actor_manager.active_sessions().await
    }
    
    /// 获取会话连接信息
    pub async fn session_info(&self, session_id: SessionId) -> Result<ConnectionInfo, TransportError> {
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.connection_info().await
        } else {
            Err(TransportError::connection_error("Session not found or already closed", false))
        }
    }
    
    /// 获取传输统计信息
    pub async fn stats(&self) -> Result<HashMap<SessionId, TransportStats>, TransportError> {
        let sessions = self.actor_manager.active_sessions().await;
        let mut stats = HashMap::new();
        
        for session_id in sessions {
            if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
                if let Ok(session_stats) = handle.stats().await {
                    stats.insert(session_id, session_stats);
                }
            }
        }
        
        Ok(stats)
    }
    
    /// 获取事件流
    pub fn events(&self) -> EventStream {
        EventStream::new(self.actor_manager.global_events())
    }
    
    /// 获取特定会话的事件流
    pub fn session_events(&self, session_id: SessionId) -> EventStream {
        EventStream::with_session_filter(self.actor_manager.global_events(), session_id)
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
        
        tracing::info!("✅ 所有 {} 个协议服务器启动完成，开始事件处理循环", server_handles.len());
        
        // 启动事件处理循环
        let mut events = self.events();
        let event_handle = tokio::spawn(async move {
            while let Some(event) = events.next().await {
                match event {
                    Event::ConnectionEstablished { session_id, info } => {
                        tracing::debug!("🔗 新连接建立: {} [{:?}]", session_id, info.protocol);
                    }
                    Event::ConnectionClosed { session_id, reason } => {
                        tracing::debug!("❌ 连接关闭: {} - {:?}", session_id, reason);
                    }
                    Event::MessageReceived { session_id, packet } => {
                        tracing::trace!("📨 收到消息 (会话 {}): {:?}", session_id, packet);
                    }
                    Event::MessageSent { session_id, packet_id } => {
                        tracing::trace!("📤 发送消息 (会话 {}): packet_id={}", session_id, packet_id);
                    }
                    _ => {}
                }
            }
        });
        
        // 等待所有任务完成（实际上是永远运行）
        server_handles.push(event_handle);
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
        if let Some(handle) = self.actor_manager.get_actor(&session_id).await {
            handle.close().await?;
            self.actor_manager.remove_actor(&session_id).await;
            tracing::debug!("👋 会话 {} 已关闭", session_id);
            Ok(())
        } else {
            // 会话不存在，可能已经被自动清理，这是正常情况
            tracing::debug!("👋 会话 {} 已经关闭或不存在，跳过关闭操作", session_id);
            Ok(())
        }
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
        let actor_manager = Arc::new(ActorManager::new());
        let event_stream = EventStream::new(actor_manager.global_events());
        
        // 创建标准协议注册表
        let protocol_registry = Arc::new(create_standard_registry().await?);
        
        // 🚀 Phase 3: 创建优化后的高性能组件
        let smart_pool = expert_config.smart_pool.unwrap_or_default();
        let performance = expert_config.performance.unwrap_or_default();
        
        let connection_pool = Arc::new(
            ConnectionPool::new(
                smart_pool.initial_size,
                smart_pool.max_size
            ).initialize_pool().await?
        );
        
        let memory_pool = Arc::new(
            OptimizedMemoryPool::new()
                .with_preallocation(
                    1000,  // 默认缓存大小
                    500,   
                    250
                )
        );
        
        tracing::info!("🚀 Transport 创建成功，默认启用高性能组件:");
        tracing::info!("   ✅ LockFree 连接池 (初始: {}, 最大: {})", 
                      smart_pool.initial_size,
                      smart_pool.max_size);
        tracing::info!("   ✅ 优化内存池 (缓存: 1000)");
        tracing::info!("   ✅ 详细监控: {}", performance.enable_detailed_monitoring);
        
        Ok(Transport {
            actor_manager,
            event_stream,
            session_id_generator: Arc::new(AtomicU64::new(1)),
            config,
            protocol_registry,
            configured_servers,
            connection_pool,
            memory_pool,
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
            actor_manager: self.actor_manager.clone(),
            event_stream: EventStream::new(self.actor_manager.global_events()),
            session_id_generator: self.session_id_generator.clone(),
            config: self.config.clone(),
            protocol_registry: self.protocol_registry.clone(),
            configured_servers: Vec::new(), // Clone时不复制服务器，因为它们已经被消费了
            connection_pool: self.connection_pool.clone(),
            memory_pool: self.memory_pool.clone(),
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