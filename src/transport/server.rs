/// 服务端传输层模块
/// 
/// 提供专门针对服务端监听的传输层API

use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use std::collections::HashMap;
use crossbeam_channel::{unbounded as crossbeam_unbounded, Receiver as CrossbeamReceiver, Sender as CrossbeamSender};

use crate::{
    SessionId,
    error::TransportError,
    transport::{api::Transport, config::TransportConfig, lockfree_enhanced::LockFreeHashMap},
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

/// 🚀 Phase 1 迁移：服务器控制命令 (同步高性能)
#[derive(Debug)]
enum ServerControlCommand {
    AddSession(SessionId, Transport),
    RemoveSession(SessionId),
    Shutdown,
}

/// 🏗️ Phase 1 迁移：混合架构服务器传输
/// 
/// 使用 LockFree + Crossbeam 的高性能会话管理
pub struct ServerTransport {
    /// ✅ 第一阶段迁移：LockFree 会话管理 (替代 Arc<RwLock<HashMap>>)
    sessions: Arc<LockFreeHashMap<SessionId, Transport>>,
    
    /// 🔧 第一阶段迁移：Crossbeam 同步控制通道 (替代 Tokio)
    control_tx: CrossbeamSender<ServerControlCommand>,
    control_rx: Option<CrossbeamReceiver<ServerControlCommand>>,
    
    /// 服务器实例管理 (保持 Tokio Mutex 用于低频操作)
    servers: Arc<Mutex<HashMap<String, Box<dyn Server>>>>,
    
    /// 服务端配置
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    
    /// 协议配置 - 用于创建服务器监听
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    
    /// 全局Actor管理器 - 所有Transport实例共享
    global_actor_manager: Arc<crate::transport::actor_v2::ActorManager>,
    
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
        use crate::transport::actor_v2::ActorManager;
        let global_actor_manager = Arc::new(ActorManager::new());
        
        // 🚀 Phase 1: 创建 Crossbeam 同步控制通道 (修复)
        let (control_tx, control_rx) = crossbeam_unbounded();
        
        Ok(Self {
            /// ✅ Phase 1: LockFree 会话管理
            sessions: Arc::new(LockFreeHashMap::new()),
            
            /// 🔧 Phase 1: Crossbeam 同步控制
            control_tx,
            control_rx: Some(control_rx),
            
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
                        
                        // 🚀 Phase 1: LockFree 会话添加 (替代 RwLock)
                        if let Err(e) = sessions.insert(session_id, transport.clone()) {
                            tracing::error!("❌ 添加会话失败 {}: {:?}", session_id, e);
                            continue;
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
                            
                            // 🚀 Phase 1: LockFree 会话检查 (替代 RwLock)
                            if sessions_for_cleanup.get(&session_id).is_some() {
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
                        
                        // 🚀 Phase 1: LockFree 会话添加 (WebSocket)
                        if let Err(e) = sessions.insert(session_id, transport) {
                            tracing::error!("❌ 添加 WebSocket 会话失败 {}: {:?}", session_id, e);
                            continue;
                        }
                        
                        tracing::info!("🎯 会话已注册 (会话ID: {})", session_id);
                        
                        // 启动连接监控任务
                        let sessions_for_cleanup = sessions.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                            
                            // 🚀 Phase 1: LockFree 会话检查 (WebSocket)
                            if sessions_for_cleanup.get(&session_id).is_some() {
                                // 会话仍然存在，执行清理
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
                        
                        // 🚀 Phase 1: LockFree 会话添加 (QUIC)
                        if let Err(e) = sessions.insert(session_id, transport) {
                            tracing::error!("❌ 添加 QUIC 会话失败 {}: {:?}", session_id, e);
                            continue;
                        }
                        
                        tracing::info!("🎯 会话已注册 (会话ID: {})", session_id);
                        
                        // 启动连接监控任务
                        let sessions_for_cleanup = sessions.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                            
                            // 🚀 Phase 1: LockFree 会话检查 (QUIC)
                            if sessions_for_cleanup.get(&session_id).is_some() {
                                // 会话仍然存在，执行清理
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
        // 🚀 Phase 1: LockFree 同步查找 (替代 RwLock::read().await)
        if let Some(transport) = self.sessions.get(&session_id) {
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
        // 🚀 Phase 1: LockFree 同步遍历 (替代 RwLock::read().await)
        let mut errors = Vec::new();
        
        self.sessions.for_each(|session_id, transport| {
            // 异步操作需要在这里处理，但我们先收集所有会话
            // 然后在外部进行异步操作
        })?;
        
        // 获取所有会话的快照进行异步处理
        let sessions_snapshot = self.sessions.snapshot()?;
        
        for (session_id, transport) in sessions_snapshot {
            if let Err(e) = transport.send_to_session(session_id, packet.clone()).await {
                errors.push((session_id, e));
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
        // 创建一个新的事件流，因为不再有全局ActorManager
        // 这里可以考虑从某个会话获取事件流，或者创建一个空的事件流
        let (tx, rx) = tokio::sync::broadcast::channel(1000);
        EventStream::new(rx)
    }
    
    /// 获取会话事件流
    pub async fn session_events(&self, session_id: SessionId) -> Result<EventStream, TransportError> {
        // 🚀 Phase 1: LockFree 同步查找
        if let Some(transport) = self.sessions.get(&session_id) {
            Ok(transport.events())
        } else {
            Err(TransportError::config_error("session", "Session not found"))
        }
    }

    /// 添加新会话 - 统一会话创建和注册逻辑
    pub async fn add_session(&self, transport: Transport) -> SessionId {
        let session_id = self.generate_session_id();
        
        // 🚀 Phase 1: LockFree 同步插入 (替代 RwLock::write().await)
        if let Err(e) = self.sessions.insert(session_id, transport) {
            tracing::error!("❌ 添加会话失败 {}: {:?}", session_id, e);
        } else {
            tracing::info!("新会话已创建: {}", session_id);
        }
        
        session_id
    }

    /// 生成新的会话ID
    fn generate_session_id(&self) -> SessionId {
        SessionId(self.session_id_generator.fetch_add(1, Ordering::Relaxed))
    }

    /// 从连接创建 Transport 实例 - 使用简化架构
    async fn create_transport_from_connection(
        connection: Box<dyn crate::protocol::protocol::Connection>,
        server_session_id: SessionId,
        _shared_actor_manager: Arc<crate::transport::actor_v2::ActorManager>, // 不再使用
    ) -> Result<Transport, TransportError> {
        use crate::transport::config::TransportConfig;
        use crate::protocol::ProtocolConnectionAdapter;
        
        tracing::debug!("🔧 为连接创建 Transport 实例并集成连接 (服务器会话ID: {})", server_session_id);
        
        // ✅ 使用简化的Transport创建
        let transport = Transport::new(TransportConfig::default()).await?;
        
        // 将连接包装为协议适配器
        let adapter = ProtocolConnectionAdapter::new(connection);
        
        // 将适配器添加到 Transport 中，获取Transport内部的会话ID
        let internal_session_id = transport.add_connection(adapter).await?;
        tracing::debug!("✅ 连接已成功集成到 Transport (服务器会话ID: {}, 内部会话ID: {})", server_session_id, internal_session_id);
        
        Ok(transport)
    }

    /// 清理会话
    async fn cleanup_session(
        sessions: &Arc<LockFreeHashMap<SessionId, Transport>>,
        session_id: SessionId,
    ) {
        // 🚀 Phase 1: LockFree 同步移除 (替代 RwLock::write().await)
        if let Ok(Some(transport)) = sessions.remove(&session_id) {
            // 尝试优雅关闭 transport 中的连接
            if let Err(e) = transport.close_session(session_id).await {
                tracing::warn!("⚠️ 关闭会话 {} 时出错: {:?}", session_id, e);
            }
            tracing::info!("🧹 会话已清理 (会话ID: {})", session_id);
        }
    }
    
    /// 获取传输统计
    pub async fn stats(&self) -> Result<std::collections::HashMap<SessionId, crate::command::TransportStats>, TransportError> {
        let mut stats = std::collections::HashMap::new();
        
        // 🚀 Phase 1: LockFree 遍历 (替代 RwLock::read().await)
        let sessions_snapshot = self.sessions.snapshot()?;
        
        for (session_id, transport) in sessions_snapshot {
            if let Ok(transport_stats) = transport.stats().await {
                // 合并传输统计，取第一个匹配的会话统计
                if let Some(session_stats) = transport_stats.get(&session_id) {
                    stats.insert(session_id, session_stats.clone());
                }
            }
        }
        
        Ok(stats)
    }
    
    /// 获取活跃会话
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        // 🚀 Phase 1: LockFree 键遍历 (替代 RwLock::read().await)
        self.sessions.keys().unwrap_or_default()
    }
    
    /// 关闭指定会话
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        // 🚀 Phase 1: LockFree 同步移除 (替代 RwLock::write().await)
        if let Ok(Some(transport)) = self.sessions.remove(&session_id) {
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
            control_tx: self.control_tx.clone(),
            control_rx: None,
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