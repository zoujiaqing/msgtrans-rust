/// 服务端传输层模块
/// 
/// 提供专门针对服务端监听的传输层API

use std::time::Duration;

use crate::{
    SessionId,
    error::TransportError,
    transport::config::TransportConfig,
    protocol::adapter::ServerConfig,
};

// 导入新的 TransportServer
use super::transport_server::TransportServer;

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
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynServerConfig>>,
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
    pub fn with_protocol<T: crate::protocol::adapter::DynServerConfig>(mut self, config: T) -> Self {
        let protocol_name = config.protocol_name().to_string();
        self.protocol_configs.insert(protocol_name, Box::new(config));
        self
    }
    
    /// 构建服务端传输层 - 返回 TransportServer
    pub async fn build(self) -> Result<TransportServer, TransportError> {
        // 创建基础配置并构建新的 TransportServer，传递协议配置
        let transport_config = self.transport_config.clone();
        let protocol_configs = self.protocol_configs;
        
        let transport_server = super::transport_server::TransportServer::new_with_protocols(
            transport_config, 
            protocol_configs
        ).await?;
        
        tracing::info!("✅ TransportServer 构建完成，已包含协议配置");
        Ok(transport_server)
    }
}

impl Default for TransportServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/* 🚧 TODO: 旧的 ServerTransport 实现 - 等待重构或移除
/// 🚀 Phase 1 迁移：服务器控制命令 (同步高性能)
#[derive(Debug)]
enum ServerControlCommand {
    AddSession(SessionId, Transport),
    RemoveSession(SessionId),
    Shutdown,
}

/// 🏗️ Phase 1 迁移：混合架构服务器传输
/// 
/// 使用优化的数据结构和传统的结构化代码：
/// - ✅ LockFree 会话管理 (HashMap 替换)
/// - ✅ Crossbeam 同步控制通道 (Actor系统简化)
/// - ✅ 统一Transport接口
pub struct ServerTransport {
    /// ✅ 第一阶段迁移：LockFree 会话管理 (替代 Arc<RwLock<HashMap>>)
    sessions: Arc<LockFreeHashMap<SessionId, Transport>>,
    
    /// 🔧 第一阶段迁移：Crossbeam 同步控制通道 (替代 Tokio)
    control_tx: CrossbeamSender<ServerControlCommand>,
    control_rx: Option<CrossbeamReceiver<ServerControlCommand>>,
    
    // 🎯 临时移除：统一的会话管理器（支持事件流）
    // session_manager: Arc<SimplifiedSessionManager>,
    
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

// ... 其余的 ServerTransport 实现都被注释掉 ...
*/

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