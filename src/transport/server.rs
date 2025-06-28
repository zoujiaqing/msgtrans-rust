/// Server-side transport layer module
/// 
/// Provides transport layer API specifically for server-side listening

use std::time::Duration;

use crate::{
    SessionId,
    error::TransportError,
    transport::config::TransportConfig,
    protocol::adapter::ServerConfig,
};

// 导入新的 TransportServer
use super::transport_server::TransportServer;

/// Acceptor configuration
#[derive(Debug, Clone)]
pub struct AcceptorConfig {
    pub threads: usize,
    pub backpressure: BackpressureStrategy,
    pub accept_timeout: Duration,
}

impl Default for AcceptorConfig {
    fn default() -> Self {
        Self {
            threads: 4, // Default 4 threads, avoid num_cpus dependency
            backpressure: BackpressureStrategy::Block,
            accept_timeout: Duration::from_secs(1),
        }
    }
}

/// Backpressure strategy
#[derive(Debug, Clone)]
pub enum BackpressureStrategy {
    Block,
    DropOldest,
    DropNewest,
    Reject,
}

/// Rate limiter configuration
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

/// Server middleware trait
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

/// Server-side transport builder - focused on server listening related configuration
pub struct TransportServerBuilder {
    bind_timeout: Duration,
    max_connections: usize,
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    transport_config: TransportConfig,
    /// Protocol configuration storage - server supports multi-protocol listening
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
    
    /// Server-specific: bind timeout
    pub fn bind_timeout(mut self, timeout: Duration) -> Self {
        self.bind_timeout = timeout;
        self
    }
    
    /// Server-specific: maximum connections
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// Server-specific: acceptor thread count
    pub fn acceptor_threads(mut self, threads: usize) -> Self {
        self.acceptor_config.threads = threads;
        self
    }
    
    /// Server-specific: backpressure strategy
    pub fn backpressure_strategy(mut self, strategy: BackpressureStrategy) -> Self {
        self.acceptor_config.backpressure = strategy;
        self
    }
    
    /// Server-specific: rate limiter
    pub fn rate_limiter(mut self, config: RateLimiterConfig) -> Self {
        self.rate_limiter = Some(config);
        self
    }
    
    /// Server-specific: middleware
    pub fn with_middleware<M: ServerMiddleware + 'static>(mut self, middleware: M) -> Self {
        self.middleware_stack.push(Box::new(middleware));
        self
    }
    
    /// Server-specific: graceful shutdown timeout
    pub fn graceful_shutdown(mut self, timeout: Option<Duration>) -> Self {
        self.graceful_shutdown = timeout;
        self
    }
    
    /// Set transport layer base configuration
    pub fn transport_config(mut self, config: TransportConfig) -> Self {
        self.transport_config = config;
        self
    }
    
    /// Unified protocol configuration interface - server supports multi-protocol
    pub fn with_protocol<T: crate::protocol::adapter::DynServerConfig>(mut self, config: T) -> Self {
        let protocol_name = config.protocol_name().to_string();
        self.protocol_configs.insert(protocol_name, Box::new(config));
        self
    }
    
    /// Build server-side transport layer - returns TransportServer
    pub async fn build(self) -> Result<TransportServer, TransportError> {
        // Create base configuration and build new TransportServer, pass protocol configurations
        let transport_config = self.transport_config.clone();
        let protocol_configs = self.protocol_configs;
        
        let transport_server = super::transport_server::TransportServer::new_with_protocols(
            transport_config, 
            protocol_configs
        ).await?;
        
        tracing::info!("[SUCCESS] TransportServer build completed, protocol configurations included");
        Ok(transport_server)
    }
}

impl Default for TransportServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/* TODO: Old ServerTransport implementation - awaiting refactoring or removal
/// Server control command (synchronous high-performance)
#[derive(Debug)]
enum ServerControlCommand {
    AddSession(SessionId, Transport),
    RemoveSession(SessionId),
    Shutdown,
}

/// Hybrid architecture server transport
/// 
/// Uses optimized data structures and traditional structured code:
/// - Lock-free session management (HashMap replacement)
/// - Crossbeam synchronous control channel (Actor system simplification)
/// - Unified Transport interface
pub struct ServerTransport {
    /// Lock-free session management (replacement for Arc<RwLock<HashMap>>)
    sessions: Arc<LockFreeHashMap<SessionId, Transport>>,
    
    /// Crossbeam synchronous control channel (replacement for Tokio)
    control_tx: CrossbeamSender<ServerControlCommand>,
    control_rx: Option<CrossbeamReceiver<ServerControlCommand>>,
    
    // Temporarily removed: unified session manager (with event stream support)
    // session_manager: Arc<SimplifiedSessionManager>,
    
    /// Server instance management (keep Tokio Mutex for low-frequency operations)
    servers: Arc<Mutex<HashMap<String, Box<dyn Server>>>>,
    
    /// Server configuration
    acceptor_config: AcceptorConfig,
    rate_limiter: Option<RateLimiterConfig>,
    middleware_stack: Vec<Box<dyn ServerMiddleware>>,
    graceful_shutdown: Option<Duration>,
    
    /// Protocol configuration - for creating server listeners
    protocol_configs: std::collections::HashMap<String, Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    
    /// Global Actor manager - shared by all Transport instances
    global_actor_manager: Arc<crate::transport::actor::ActorManager>,
    
    /// Session ID generator
    session_id_generator: Arc<AtomicU64>,
}

// ... Rest of ServerTransport implementation is commented out ...
*/

/// Example middleware implementations

/// Logging middleware
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

/// Authentication middleware
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
            tracing::debug!("Authentication check: {:?}", session_id);
            // Specific authentication logic can be implemented here
        }
        Ok(())
    }
} 