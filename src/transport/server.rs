/// 服务端传输层模块
/// 
/// 提供专门针对服务端监听的传输层API

use std::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;
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
    
    /// 构建服务端传输层
    pub async fn build(mut self) -> Result<ServerTransport, TransportError> {
        let core_transport = self.build_core_transport().await?;
        
        Ok(ServerTransport::new(
            core_transport,
            self.acceptor_config,
            self.rate_limiter,
            std::mem::take(&mut self.middleware_stack),
            self.graceful_shutdown,
            self.protocol_configs,
        ))
    }
    
    async fn build_core_transport(&self) -> Result<Transport, TransportError> {
        // 重用现有的Transport构建逻辑
        use crate::transport::api::TransportBuilder;
        
        TransportBuilder::new()
            .config(self.transport_config.clone())
            .build()
            .await
    }
}

impl Default for TransportServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 服务端传输层
/// 
/// 专注于服务监听和连接管理
pub struct ServerTransport {
    inner: Transport,
    servers: Arc<Mutex<HashMap<String, Box<dyn Server>>>>,
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    /// 协议配置存储 - 服务端支持多协议监听
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
}

impl ServerTransport {
    pub(crate) fn new(
        transport: Transport,
        acceptor_config: AcceptorConfig,
        rate_limiter: Option<RateLimiterConfig>,
        middleware_stack: Vec<Box<dyn ServerMiddleware>>,
        graceful_shutdown: Option<Duration>,
        protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    ) -> Self {
        Self {
            inner: transport,
            servers: Arc::new(Mutex::new(HashMap::new())),
            acceptor_config,
            rate_limiter,
            middleware_stack,
            graceful_shutdown,
            protocol_configs,
        }
    }
    
    /// 🚀 流式API入口 - 核心功能
    pub fn with_protocol<C>(&self, config: C) -> ProtocolServerBuilder<'_, C>
    where
        C: ProtocolConfig + ServerConfig,
    {
        ProtocolServerBuilder::new(self, config)
    }
    
    /// 🚀 快速启动服务 - 使用构建时的所有协议配置
    pub async fn serve_all(&self) -> Result<Vec<String>, TransportError> {
        let mut server_ids: Vec<String> = Vec::new();
        
        for (protocol_name, _protocol_config) in &self.protocol_configs {
            // 这里需要动态分发到具体的协议
            // 暂时返回一个错误，提示需要具体实现
            tracing::info!("准备启动 {} 协议服务器", protocol_name);
            // TODO: 实现动态协议服务器创建
        }
        
        if self.protocol_configs.is_empty() {
            Err(TransportError::config_error("protocol", "No protocols configured - use with_protocol() to add protocols"))
        } else {
            Err(TransportError::config_error("protocol", "Quick serve not yet implemented - use with_protocol() instead"))
        }
    }
    
    /// 启动多协议服务器
    pub async fn serve_multiple<C>(&self, configs: Vec<C>) -> Result<Vec<String>, TransportError>
    where
        C: ProtocolConfig + ServerConfig + Clone,
    {
        let mut server_ids = Vec::new();
        for config in configs {
            let server_id = self.with_protocol(config).serve().await?;
            server_ids.push(server_id);
        }
        Ok(server_ids)
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
    
    // 委托给内部transport的通用方法
    pub fn events(&self) -> EventStream {
        self.inner.events()
    }
    
    pub fn session_events(&self, session_id: SessionId) -> EventStream {
        self.inner.session_events(session_id)
    }
    
    /// 获取传输统计
    pub async fn stats(&self) -> Result<std::collections::HashMap<SessionId, crate::command::TransportStats>, TransportError> {
        self.inner.stats().await
    }
    
    /// 获取活跃会话
    pub async fn active_sessions(&self) -> Result<Vec<SessionId>, TransportError> {
        Ok(self.inner.active_sessions().await)
    }
}

/// 服务端构建器 - 流式API实现
pub struct ProtocolServerBuilder<'t, C> {
    transport: &'t ServerTransport,
    config: C,
    server_options: ServerOptions,
}

impl<'t, C> ProtocolServerBuilder<'t, C>
where
    C: ProtocolConfig + ServerConfig,
{
    pub(crate) fn new(transport: &'t ServerTransport, config: C) -> Self {
        Self { 
            transport, 
            config,
            server_options: ServerOptions::default(),
        }
    }
    
    /// 设置服务器名称
    pub fn with_name(mut self, name: String) -> Self {
        self.server_options.name = Some(name);
        self
    }
    
    /// 设置最大连接数
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.server_options.max_connections = Some(max);
        self
    }
    
    /// 设置空闲超时
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Self {
        self.server_options.idle_timeout = Some(timeout);
        self
    }
    
    /// 🎯 启动服务器 - 核心方法
    pub async fn serve(self) -> Result<String, TransportError> {
        // 使用ServerConfig的validate避免冲突
        use crate::protocol::adapter::ServerConfig;
        ServerConfig::validate(&self.config)
            .map_err(|e| TransportError::config_error("protocol", format!("Config validation failed: {:?}", e)))?;
        
        tracing::info!("启动 {} 服务器", self.config.protocol_name());
        
        // 构建服务器
        let server = self.config.build_server().await?;
        
        // 生成服务器ID - 使用简单的UUID实现
        let server_id = self.server_options.name
            .unwrap_or_else(|| format!("{}-server-{}", 
                self.config.protocol_name(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            ));
        
        // 应用中间件
        for middleware in &self.transport.middleware_stack {
            tracing::debug!("应用中间件: {}", middleware.name());
        }
        
        // 注册服务器
        let mut servers = self.transport.servers.lock().await;
        servers.insert(server_id.clone(), Box::new(server));
        
        tracing::info!("✅ {} 服务器启动成功: {}", self.config.protocol_name(), server_id);
        
        Ok(server_id)
    }
}

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