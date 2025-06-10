/// 服务端传输层模块
/// 
/// 提供专门针对服务端监听的传输层API

use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::{Mutex, RwLock};
use std::collections::HashMap;

use crate::{
    SessionId,
    error::TransportError,
    transport::{api::Transport, config::TransportConfig},
    protocol::{ProtocolConfig, adapter::ServerConfig, protocol::Server},
    stream::EventStream,
};

/// 接受器配置
#[derive(Debug, Clone)]
pub struct AcceptorConfig {
    pub threads: usize,
    pub backpressure: BackpressureStrategy,
    pub accept_timeout: Duration,
}

impl Default for AcceptorConfig {
    fn default() -> Self {
        Self {
            threads: 4, // 默认4个线程，避免num_cpus依赖
            backpressure: BackpressureStrategy::Block,
            accept_timeout: Duration::from_secs(1),
        }
    }
}

/// 背压策略
#[derive(Debug, Clone)]
pub enum BackpressureStrategy {
    Block,
    DropOldest,
    DropNewest,
    Reject,
}

/// 限流配置
#[derive(Debug, Clone)]
pub struct RateLimiterConfig {
    pub requests_per_second: u32,
    pub burst_size: u32,
    pub window_size: Duration,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 1000,
            burst_size: 100,
            window_size: Duration::from_secs(1),
        }
    }
}

/// 服务器中间件trait
pub trait ServerMiddleware: Send + Sync {
    fn name(&self) -> &'static str;
    fn process(&self, session_id: SessionId) -> Result<(), TransportError>;
}

/// 服务器选项
#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub name: Option<String>,
    pub max_connections: Option<usize>,
    pub idle_timeout: Option<Duration>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            name: None,
            max_connections: None,
            idle_timeout: Some(Duration::from_secs(300)),
        }
    }
}

/// 服务端传输构建器 - 专注于服务监听相关配置
pub struct TransportServerBuilder {
    bind_timeout: Duration,
    max_connections: usize,
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    transport_config: TransportConfig,
    /// 协议配置存储 - 服务端支持多协议监听
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
}

impl TransportServerBuilder {
    pub fn new() -> Self {
        Self {
            bind_timeout: Duration::from_secs(10),
            max_connections: 10000,
            acceptor_config: AcceptorConfig::default(),
            rate_limiter: None,
            middleware_stack: Vec::new(),
            graceful_shutdown: Some(Duration::from_secs(30)),
            transport_config: TransportConfig::default(),
            protocol_configs: std::collections::HashMap::new(),
        }
    }
    
    /// 服务端专用：绑定超时
    pub fn bind_timeout(mut self, timeout: Duration) -> Self {
        self.bind_timeout = timeout;
        self
    }
    
    /// 服务端专用：最大连接数
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// 服务端专用：接受器线程数
    pub fn acceptor_threads(mut self, threads: usize) -> Self {
        self.acceptor_config.threads = threads;
        self
    }
    
    /// 服务端专用：背压策略
    pub fn backpressure_strategy(mut self, strategy: BackpressureStrategy) -> Self {
        self.acceptor_config.backpressure = strategy;
        self
    }
    
    /// 服务端专用：限流
    pub fn rate_limiter(mut self, config: RateLimiterConfig) -> Self {
        self.rate_limiter = Some(config);
        self
    }
    
    /// 服务端专用：中间件
    pub fn with_middleware<M: ServerMiddleware + 'static>(mut self, middleware: M) -> Self {
        self.middleware_stack.push(Box::new(middleware));
        self
    }
    
    /// 服务端专用：优雅关闭超时
    pub fn graceful_shutdown(mut self, timeout: Option<Duration>) -> Self {
        self.graceful_shutdown = timeout;
        self
    }
    
    /// 设置传输层基础配置
    pub fn transport_config(mut self, config: TransportConfig) -> Self {
        self.transport_config = config;
        self
    }
    
    /// 🌟 统一协议配置接口 - 服务端支持多协议
    pub fn with_protocol<T: crate::protocol::adapter::DynProtocolConfig>(mut self, config: T) -> Self {
        let protocol_name = config.protocol_name().to_string();
        self.protocol_configs.insert(protocol_name, Box::new(config));
        self
    }
    
    /// 构建服务端传输层 - 返回 ServerTransport
    pub async fn build(mut self) -> Result<ServerTransport, TransportError> {
        Ok(ServerTransport::new(
            self.acceptor_config,
            self.rate_limiter,
            std::mem::take(&mut self.middleware_stack),
            self.graceful_shutdown,
            self.protocol_configs,
        ).await?)
    }
    

}

impl Default for TransportServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 服务端传输层
/// 
/// 管理多个客户端连接，每个连接对应一个 Transport 实例
pub struct ServerTransport {
    /// ✅ 核心设计：管理多个 Transport 实例 (每个客户端连接一个)
    sessions: Arc<RwLock<HashMap<SessionId, Transport>>>,
    /// 服务器实例管理
    servers: Arc<Mutex<HashMap<String, Box<dyn Server>>>>,
    /// 服务端配置
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    /// 协议配置 - 用于创建服务器监听
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    /// 全局Actor管理器 - 所有Transport实例共享
    global_actor_manager: Arc<crate::actor::ActorManager>,
    /// 会话ID生成器
    session_id_generator: Arc<AtomicU64>,
}

impl ServerTransport {
    pub(crate) async fn new(
        acceptor_config: AcceptorConfig,
        rate_limiter: Option<RateLimiterConfig>,
        middleware_stack: Vec<Box<dyn ServerMiddleware>>,
        graceful_shutdown: Option<Duration>,
        protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    ) -> Result<Self, TransportError> {
        // 创建全局Actor管理器
        use crate::actor::ActorManager;
        let global_actor_manager = Arc::new(ActorManager::new());
        
        Ok(Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            servers: Arc::new(Mutex::new(HashMap::new())),
            acceptor_config,
            rate_limiter,
            middleware_stack,
            graceful_shutdown,
            protocol_configs,
            global_actor_manager,
            session_id_generator: Arc::new(AtomicU64::new(1)),
        })
    }
    
    /// 🚀 启动服务器 - 根据协议配置启动多协议监听
    pub async fn serve(self) -> Result<(), TransportError> {
        if self.protocol_configs.is_empty() {
            tracing::warn!("⚠️ 没有配置任何协议，服务器将无法接受连接");
            return Ok(());
        }
        
        tracing::info!("🌟 启动 {} 个协议服务器", self.protocol_configs.len());
        
        // 用于存储服务器句柄和任务
        let mut server_tasks = Vec::new();
        
        // 启动所有协议服务器
        for (protocol_name, protocol_config) in &self.protocol_configs {
            tracing::info!("📡 启动 {} 协议服务器", protocol_name);
            
            // 根据协议类型启动对应的服务器
            match protocol_name.as_str() {
                "tcp" => {
                    if let Some(tcp_config) = protocol_config.as_any().downcast_ref::<crate::protocol::TcpServerConfig>() {
                        let server_task = self.start_tcp_server(tcp_config.clone()).await?;
                        server_tasks.push(server_task);
                        tracing::info!("✅ TCP 服务器启动成功: {}", tcp_config.bind_address);
                    }
                }
                #[cfg(feature = "websocket")]
                "websocket" => {
                    if let Some(ws_config) = protocol_config.as_any().downcast_ref::<crate::protocol::WebSocketServerConfig>() {
                        let server_task = self.start_websocket_server(ws_config.clone()).await?;
                        server_tasks.push(server_task);
                        tracing::info!("✅ WebSocket 服务器启动成功: {}", ws_config.bind_address);
                    }
                }
                #[cfg(feature = "quic")]
                "quic" => {
                    if let Some(quic_config) = protocol_config.as_any().downcast_ref::<crate::protocol::QuicServerConfig>() {
                        let server_task = self.start_quic_server(quic_config.clone()).await?;
                        server_tasks.push(server_task);
                        tracing::info!("✅ QUIC 服务器启动成功: {}", quic_config.bind_address);
                    }
                }
                _ => {
                    tracing::warn!("⚠️ 未知协议类型: {}", protocol_name);
                }
            }
        }
        
        tracing::info!("🎯 所有协议服务器启动完成，等待连接...");
        
        // 等待所有服务器任务完成
        for task in server_tasks {
            match task.await {
                Ok(Ok(())) => {
                    tracing::info!("✅ 服务器任务正常完成");
                }
                Ok(Err(e)) => {
                    tracing::error!("❌ 服务器任务出错: {:?}", e);
                    return Err(e);
                }
                Err(e) => {
                    tracing::error!("❌ 服务器任务被取消: {:?}", e);
                    return Err(TransportError::config_error("server", "Server task cancelled"));
                }
            }
        }
        
        Ok(())
    }
    
    /// 启动 TCP 服务器
    async fn start_tcp_server(
        &self,
        config: crate::protocol::TcpServerConfig,
    ) -> Result<tokio::task::JoinHandle<Result<(), TransportError>>, TransportError> {
        use crate::protocol::adapter::ServerConfig;
        
        // 构建 TCP 服务器
        let mut server = config.build_server().await?;
        let bind_addr = config.bind_address;
        
        // 获取需要传入异步任务的引用
        let sessions = self.sessions.clone();
        let session_id_generator = self.session_id_generator.clone();
        let global_actor_manager = self.global_actor_manager.clone();
        
        let task = tokio::spawn(async move {
            tracing::info!("🌐 TCP 服务器监听: {}", bind_addr);
            
            // 真正的 TCP 连接接受循环
            loop {
                match server.accept().await {
                    Ok(mut connection) => {
                        // 生成新的会话ID
                        let session_id = SessionId(session_id_generator.fetch_add(1, Ordering::Relaxed));
                        
                        // 设置连接的会话ID
                        connection.set_session_id(session_id);
                        
                        tracing::info!("✅ TCP 新连接接受 (会话ID: {})", session_id);
                        
                        // 从连接创建 Transport 实例
                        let transport = match Self::create_transport_from_connection(connection, session_id, global_actor_manager.clone()).await {
                            Ok(transport) => transport,
                            Err(e) => {
                                tracing::error!("❌ 创建 Transport 失败 (会话ID: {}): {:?}", session_id, e);
                                continue;
                            }
                        };
                        
                        // 将新的 Transport 加入 sessions
                        {
                            let mut sessions_guard = sessions.write().await;
                            sessions_guard.insert(session_id, transport.clone());
                        }
                        
                        tracing::info!("🎯 会话已注册 (会话ID: {})", session_id);
                        
                        tracing::info!("✅ 连接处理完成 (会话ID: {})", session_id);
                        
                        // 启动连接监控任务，处理连接断开时的清理
                        let sessions_for_cleanup = sessions.clone();
                        tokio::spawn(async move {
                            // 这里可以添加连接监控逻辑
                            // 例如：定期检查连接状态，或者监听连接关闭事件
                            
                            // 暂时实现：等待一段时间后检查连接状态
                            // 在真实实现中，应该监听连接的事件或状态变化
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await; // 5分钟超时检查
                            
                            // 检查会话是否还存在，如果不存在则说明已经被清理
                            let sessions_guard = sessions_for_cleanup.read().await;
                            if sessions_guard.contains_key(&session_id) {
                                drop(sessions_guard);
                                // 会话仍然存在，执行清理
                                Self::cleanup_session(&sessions_for_cleanup, session_id).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("❌ TCP 接受连接失败: {:?}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await; // 短暂延迟后重试
                    }
                }
            }
        });
        
        Ok(task)
    }
    
    /// 启动 WebSocket 服务器
    #[cfg(feature = "websocket")]
    async fn start_websocket_server(
        &self,
        config: crate::protocol::WebSocketServerConfig,
    ) -> Result<tokio::task::JoinHandle<Result<(), TransportError>>, TransportError> {
        use crate::protocol::adapter::ServerConfig;
        
        // 构建 WebSocket 服务器
        let mut server = config.build_server().await?;
        let bind_addr = config.bind_address;
        
        // 获取需要传入异步任务的引用
        let sessions = self.sessions.clone();
        let session_id_generator = self.session_id_generator.clone();
        let global_actor_manager = self.global_actor_manager.clone();
        
        let task = tokio::spawn(async move {
            tracing::info!("🌐 WebSocket 服务器监听: {}", bind_addr);
            
            // 真正的 WebSocket 连接接受循环
            loop {
                match server.accept().await {
                    Ok(mut connection) => {
                        // 生成新的会话ID
                        let session_id = SessionId(session_id_generator.fetch_add(1, Ordering::Relaxed));
                        
                        // 设置连接的会话ID
                        connection.set_session_id(session_id);
                        
                        tracing::info!("✅ WebSocket 新连接接受 (会话ID: {})", session_id);
                        
                        // 从连接创建 Transport 实例
                        let transport = match Self::create_transport_from_connection(connection, session_id, global_actor_manager.clone()).await {
                            Ok(transport) => transport,
                            Err(e) => {
                                tracing::error!("❌ 创建 Transport 失败 (会话ID: {}): {:?}", session_id, e);
                                continue;
                            }
                        };
                        
                        // 将新的 Transport 加入 sessions
                        {
                            let mut sessions_guard = sessions.write().await;
                            sessions_guard.insert(session_id, transport);
                        }
                        
                        tracing::info!("🎯 会话已注册 (会话ID: {})", session_id);
                        
                        // 启动连接监控任务
                        let sessions_for_cleanup = sessions.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                            
                            let sessions_guard = sessions_for_cleanup.read().await;
                            if sessions_guard.contains_key(&session_id) {
                                drop(sessions_guard);
                                Self::cleanup_session(&sessions_for_cleanup, session_id).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("❌ WebSocket 接受连接失败: {:?}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });
        
        Ok(task)
    }
    
    /// 启动 QUIC 服务器
    #[cfg(feature = "quic")]
    async fn start_quic_server(
        &self,
        config: crate::protocol::QuicServerConfig,
    ) -> Result<tokio::task::JoinHandle<Result<(), TransportError>>, TransportError> {
        use crate::protocol::adapter::ServerConfig;
        
        // 构建 QUIC 服务器
        let mut server = config.build_server().await?;
        let bind_addr = config.bind_address;
        
        // 获取需要传入异步任务的引用
        let sessions = self.sessions.clone();
        let session_id_generator = self.session_id_generator.clone();
        let global_actor_manager = self.global_actor_manager.clone();
        
        let task = tokio::spawn(async move {
            tracing::info!("🌐 QUIC 服务器监听: {}", bind_addr);
            
            // 真正的 QUIC 连接接受循环
            loop {
                match server.accept().await {
                    Ok(mut connection) => {
                        // 生成新的会话ID
                        let session_id = SessionId(session_id_generator.fetch_add(1, Ordering::Relaxed));
                        
                        // 设置连接的会话ID
                        connection.set_session_id(session_id);
                        
                        tracing::info!("✅ QUIC 新连接接受 (会话ID: {})", session_id);
                        
                        // 从连接创建 Transport 实例
                        let transport = match Self::create_transport_from_connection(connection, session_id, global_actor_manager.clone()).await {
                            Ok(transport) => transport,
                            Err(e) => {
                                tracing::error!("❌ 创建 Transport 失败 (会话ID: {}): {:?}", session_id, e);
                                continue;
                            }
                        };
                        
                        // 将新的 Transport 加入 sessions
                        {
                            let mut sessions_guard = sessions.write().await;
                            sessions_guard.insert(session_id, transport);
                        }
                        
                        tracing::info!("🎯 会话已注册 (会话ID: {})", session_id);
                        
                        // 启动连接监控任务
                        let sessions_for_cleanup = sessions.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                            
                            let sessions_guard = sessions_for_cleanup.read().await;
                            if sessions_guard.contains_key(&session_id) {
                                drop(sessions_guard);
                                Self::cleanup_session(&sessions_for_cleanup, session_id).await;
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("❌ QUIC 接受连接失败: {:?}", e);
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                }
            }
        });
        
        Ok(task)
    }
    
    /// 发送消息到指定会话
    pub async fn send_to_session(
        &self,
        session_id: SessionId,
        packet: crate::packet::Packet,
    ) -> Result<(), TransportError> {
        let sessions = self.sessions.read().await;
        
        if let Some(transport) = sessions.get(&session_id) {
            // ✅ 关键修复：获取Transport的活跃会话列表，然后发送到第一个（通常只有一个）
            let active_sessions = transport.active_sessions().await;
            if let Some(&internal_session_id) = active_sessions.first() {
                tracing::debug!("🔄 发送消息：服务器会话ID {} -> Transport内部会话ID {}", session_id, internal_session_id);
                transport.send_to_session(internal_session_id, packet).await
            } else {
                tracing::error!("❌ Transport没有活跃会话 (服务器会话ID: {})", session_id);
                Err(TransportError::config_error("session", "No active internal sessions"))
            }
        } else {
            tracing::error!("❌ 服务器会话未找到: {}", session_id);
            Err(TransportError::config_error("session", "Session not found"))
        }
    }
    
    /// 广播消息到所有活跃会话
    pub async fn broadcast(&self, packet: crate::packet::Packet) -> Result<(), TransportError> {
        let sessions = self.sessions.read().await;
        let mut errors = Vec::new();
        
        for (session_id, transport) in sessions.iter() {
            if let Err(e) = transport.send_to_session(*session_id, packet.clone()).await {
                errors.push((*session_id, e));
            }
        }
        
        if errors.is_empty() {
            Ok(())
        } else {
            tracing::warn!("广播时部分会话发送失败: {:?}", errors);
            // 返回第一个错误，但记录所有错误
            Err(errors.into_iter().next().unwrap().1)
        }
    }
    
    /// 停止特定服务器
    pub async fn stop_server(&self, server_id: &str) -> Result<(), TransportError> {
        let mut servers = self.servers.lock().await;
        if let Some(mut server) = servers.remove(server_id) {
            server.shutdown().await?;
            tracing::info!("服务器已停止: {}", server_id);
        } else {
            tracing::warn!("服务器不存在: {}", server_id);
        }
        Ok(())
    }
    
    /// 停止所有服务器
    pub async fn stop_all(&self) -> Result<(), TransportError> {
        let mut servers = self.servers.lock().await;
        let server_count = servers.len();
        
        for (server_id, mut server) in servers.drain() {
            if let Err(e) = server.shutdown().await {
                tracing::error!("停止服务器 {} 时出错: {:?}", server_id, e);
            } else {
                tracing::info!("服务器已停止: {}", server_id);
            }
        }
        
        tracing::info!("所有服务器已停止 (共 {} 个)", server_count);
        Ok(())
    }
    
    /// 优雅关闭所有服务器
    pub async fn graceful_shutdown(&self) -> Result<(), TransportError> {
        if let Some(timeout) = self.graceful_shutdown {
            tracing::info!("开始优雅关闭，超时: {:?}", timeout);
            
            let shutdown_future = self.stop_all();
            
            match tokio::time::timeout(timeout, shutdown_future).await {
                Ok(result) => result,
                Err(_) => {
                    tracing::warn!("优雅关闭超时，强制停止");
                    self.stop_all().await
                }
            }
        } else {
            self.stop_all().await
        }
    }
    
    /// 列出活跃服务器
    pub async fn active_servers(&self) -> Vec<String> {
        let servers = self.servers.lock().await;
        servers.keys().cloned().collect()
    }
    
    /// 获取服务器数量
    pub async fn server_count(&self) -> usize {
        let servers = self.servers.lock().await;
        servers.len()
    }
    
    /// 获取全局事件流 - 基于共享的ActorManager
    pub fn events(&self) -> EventStream {
        // ✅ 修复：使用共享的ActorManager创建事件流
        EventStream::new(self.global_actor_manager.global_events())
    }
    
    /// 获取指定会话的事件流
    pub async fn session_events(&self, session_id: SessionId) -> Result<EventStream, TransportError> {
        let sessions = self.sessions.read().await;
        
        if let Some(transport) = sessions.get(&session_id) {
            Ok(transport.events())
        } else {
            Err(TransportError::config_error("session", "Session not found"))
        }
    }

    /// 添加新会话 - 统一会话创建和注册逻辑
    pub async fn add_session(&self, transport: Transport) -> SessionId {
        let session_id = self.generate_session_id();
        
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, transport);
        }
        
        tracing::info!("新会话已创建: {}", session_id);
        session_id
    }

    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        SessionId(self.session_id_generator.fetch_add(1, Ordering::Relaxed))
    }

    /// 从连接创建 Transport 实例 - 使用共享的ActorManager
    async fn create_transport_from_connection(
        connection: Box<dyn crate::protocol::protocol::Connection>,
        server_session_id: SessionId,
        shared_actor_manager: Arc<crate::actor::ActorManager>,
    ) -> Result<Transport, TransportError> {
        use crate::transport::config::TransportConfig;
        use crate::protocol::ProtocolConnectionAdapter;
        
        tracing::debug!("🔧 为连接创建 Transport 实例并集成连接 (服务器会话ID: {})", server_session_id);
        
        // ✅ 关键修复：使用共享的ActorManager创建Transport
        let transport = Transport::new_with_shared_actor_manager(
            TransportConfig::default(),
            shared_actor_manager
        ).await?;
        
        // 将连接包装为协议适配器
        let adapter = ProtocolConnectionAdapter::new(connection);
        
        // 将适配器添加到 Transport 中，获取Transport内部的会话ID
        let internal_session_id = transport.add_connection(adapter).await?;
        tracing::debug!("✅ 连接已成功集成到 Transport (服务器会话ID: {}, 内部会话ID: {})", server_session_id, internal_session_id);
        
        Ok(transport)
    }

    /// 清理会话
    async fn cleanup_session(
        sessions: &Arc<RwLock<HashMap<SessionId, Transport>>>,
        session_id: SessionId,
    ) {
        let mut sessions_guard = sessions.write().await;
        if let Some(transport) = sessions_guard.remove(&session_id) {
            // 尝试优雅关闭 transport 中的连接
            if let Err(e) = transport.close_session(session_id).await {
                tracing::warn!("⚠️ 关闭会话 {} 时出错: {:?}", session_id, e);
            }
            tracing::info!("🧹 会话已清理 (会话ID: {})", session_id);
        }
    }
    
    /// 获取传输统计
    pub async fn stats(&self) -> Result<std::collections::HashMap<SessionId, crate::command::TransportStats>, TransportError> {
        let sessions = self.sessions.read().await;
        let mut stats = std::collections::HashMap::new();
        
        for (session_id, transport) in sessions.iter() {
            if let Ok(transport_stats) = transport.stats().await {
                // 合并传输统计，取第一个匹配的会话统计
                if let Some(session_stats) = transport_stats.get(session_id) {
                    stats.insert(*session_id, session_stats.clone());
                }
            }
        }
        
        Ok(stats)
    }
    
    /// 获取活跃会话
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }
    
    /// 关闭指定会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        let mut sessions = self.sessions.write().await;
        
        if let Some(transport) = sessions.remove(&session_id) {
            // 释放锁后再进行可能的长时间操作
            drop(sessions);
            
            // 通过Transport关闭连接
            if let Err(e) = transport.close_session(session_id).await {
                tracing::warn!("⚠️ 关闭会话 {} 时出现错误: {:?}", session_id, e);
                // 即使关闭出错，会话也已从映射中移除，所以返回成功
            }
            
            tracing::info!("✅ 会话已关闭并移除: {}", session_id);
            Ok(())
        } else {
            Err(TransportError::config_error("session", "Session not found"))
        }
    }
}

impl Clone for ServerTransport {
    fn clone(&self) -> Self {
        Self {
            sessions: self.sessions.clone(),
            servers: self.servers.clone(),
            acceptor_config: self.acceptor_config.clone(),
            rate_limiter: self.rate_limiter.clone(),
            middleware_stack: Vec::new(), // 中间件不支持克隆，使用空向量
            graceful_shutdown: self.graceful_shutdown,
            protocol_configs: std::collections::HashMap::new(), // 协议配置不支持克隆，使用空映射
            global_actor_manager: self.global_actor_manager.clone(),
            session_id_generator: self.session_id_generator.clone(),
        }
    }
}

// ✅ 已移除错误的 ProtocolServerBuilder 设计
// 现在使用统一的 serve() 方法启动所有协议

/// 示例中间件实现

/// 日志中间件
pub struct LoggingMiddleware {
    level: tracing::Level,
}

impl LoggingMiddleware {
    pub fn new() -> Self {
        Self {
            level: tracing::Level::INFO,
        }
    }
    
    pub fn with_level(mut self, level: tracing::Level) -> Self {
        self.level = level;
        self
    }
}

impl ServerMiddleware for LoggingMiddleware {
    fn name(&self) -> &'static str {
        "logging"
    }
    
    fn process(&self, session_id: SessionId) -> Result<(), TransportError> {
        match self.level {
            tracing::Level::DEBUG => tracing::debug!("Processing session: {:?}", session_id),
            tracing::Level::INFO => tracing::info!("Processing session: {:?}", session_id),
            tracing::Level::WARN => tracing::warn!("Processing session: {:?}", session_id),
            tracing::Level::ERROR => tracing::error!("Processing session: {:?}", session_id),
            _ => tracing::trace!("Processing session: {:?}", session_id),
        }
        Ok(())
    }
}

/// 认证中间件
pub struct AuthMiddleware {
    required: bool,
}

impl AuthMiddleware {
    pub fn new() -> Self {
        Self {
            required: true,
        }
    }
    
    pub fn optional() -> Self {
        Self {
            required: false,
        }
    }
}

impl ServerMiddleware for AuthMiddleware {
    fn name(&self) -> &'static str {
        "auth"
    }
    
    fn process(&self, session_id: SessionId) -> Result<(), TransportError> {
        if self.required {
            tracing::debug!("认证检查: {:?}", session_id);
            // 这里可以实现具体的认证逻辑
        }
        Ok(())
    }
} 