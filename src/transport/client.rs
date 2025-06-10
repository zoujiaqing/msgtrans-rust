/// 客户端传输层模块
/// 
/// 提供专门针对客户端连接的传输层API

use std::time::Duration;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

use crate::{
    SessionId,
    error::TransportError,
    transport::{api::Transport, config::TransportConfig, api::ConnectableConfig},
    protocol::ProtocolConfig,
    stream::EventStream,
    packet::Packet,
};

/// 连接池配置
#[derive(Debug, Clone)]
pub struct ConnectionPoolConfig {
    pub max_size: usize,
    pub idle_timeout: Duration,
    pub health_check_interval: Duration,
    pub min_idle: usize,
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            max_size: 100,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(60),
            min_idle: 5,
        }
    }
}

/// 重试配置
#[derive(Debug, Clone)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

impl RetryConfig {
    pub fn exponential_backoff(max_retries: usize, initial_delay: Duration) -> Self {
        Self {
            max_retries,
            initial_delay,
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

/// 负载均衡配置
#[derive(Debug, Clone)]
pub enum LoadBalancerConfig {
    RoundRobin,
    Random,
    LeastConnections,
    WeightedRoundRobin(Vec<u32>),
}

/// 熔断器配置
#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub timeout: Duration,
    pub success_threshold: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            timeout: Duration::from_secs(60),
            success_threshold: 3,
        }
    }
}

/// 连接选项
#[derive(Debug, Clone)]
pub struct ConnectionOptions {
    pub timeout: Option<Duration>,
    pub max_retries: usize,
    pub priority: ConnectionPriority,
}

impl Default for ConnectionOptions {
    fn default() -> Self {
        Self {
            timeout: None,
            max_retries: 0,
            priority: ConnectionPriority::Normal,
        }
    }
}

/// 连接优先级
#[derive(Debug, Clone, Copy)]
pub enum ConnectionPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// 客户端传输构建器 - 专注于连接相关配置
pub struct TransportClientBuilder {
    connect_timeout: Duration,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    load_balancer: Option<LoadBalancerConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    connection_monitoring: bool,
    transport_config: TransportConfig,
    /// 协议配置存储 - 客户端只支持一个协议连接
    protocol_config: Option<Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
}

impl TransportClientBuilder {
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            pool_config: ConnectionPoolConfig::default(),
            retry_config: RetryConfig::default(),
            load_balancer: None,
            circuit_breaker: None,
            connection_monitoring: true,
            transport_config: TransportConfig::default(),
            protocol_config: None,
        }
    }
    
    /// 🌟 统一协议配置接口 - 客户端只支持一个协议
    pub fn with_protocol<T: crate::protocol::adapter::DynProtocolConfig>(mut self, config: T) -> Self {
        self.protocol_config = Some(Box::new(config));
        self
    }
    
    /// 客户端专用：连接超时
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    
    /// 客户端专用：连接池配置
    pub fn connection_pool(mut self, config: ConnectionPoolConfig) -> Self {
        self.pool_config = config;
        self
    }
    
    /// 客户端专用：重试策略
    pub fn retry_strategy(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }
    
    /// 客户端专用：负载均衡
    pub fn load_balancer(mut self, config: LoadBalancerConfig) -> Self {
        self.load_balancer = Some(config);
        self
    }
    
    /// 客户端专用：熔断器
    pub fn circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(config);
        self
    }
    
    /// 客户端专用：连接监控
    pub fn enable_connection_monitoring(mut self, enabled: bool) -> Self {
        self.connection_monitoring = enabled;
        self
    }
    
    /// 设置传输层基础配置
    pub fn transport_config(mut self, config: TransportConfig) -> Self {
        self.transport_config = config;
        self
    }
    
        /// 构建客户端传输层
    pub async fn build(mut self) -> Result<ClientTransport, TransportError> {
        let core_transport = self.build_core_transport().await?;
        
        Ok(ClientTransport::new(
            core_transport,
            self.pool_config,
            self.retry_config,
            self.load_balancer,
            self.circuit_breaker,
            self.protocol_config.take(),
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

impl Default for TransportClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 客户端传输层
/// 
/// 专注于建立和管理客户端连接
pub struct ClientTransport {
    inner: Transport,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    load_balancer: Option<LoadBalancerConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    // 客户端协议配置
    protocol_config: Option<Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    // 客户端专用状态
    connection_pools: Arc<RwLock<HashMap<String, ClientConnectionPool>>>,
    // 🎯 当前连接的会话ID - 对外隐藏
    current_session_id: Option<SessionId>,
}

impl ClientTransport {
    pub(crate) fn new(
        transport: Transport,
        pool_config: ConnectionPoolConfig,
        retry_config: RetryConfig,
        load_balancer: Option<LoadBalancerConfig>,
        circuit_breaker: Option<CircuitBreakerConfig>,
        protocol_config: Option<Box<dyn crate::protocol::adapter::DynProtocolConfig>>,
    ) -> Self {
        Self {
            inner: transport,
            pool_config,
            retry_config,
            load_balancer,
            circuit_breaker,
            protocol_config,
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            current_session_id: None,
        }
    }
    
    /// 🔌 流式API入口 - 核心功能
    pub fn with_protocol<C>(&self, config: C) -> ProtocolConnectionBuilder<'_, C>
    where
        C: ProtocolConfig + ConnectableConfig,
    {
        ProtocolConnectionBuilder::new(self, config)
    }
    
    /// 🚀 客户端连接 - 简化API，无需session_id
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        if let Some(protocol_config) = &self.protocol_config {
            // 使用工厂模式创建连接
            let session_id = self.inner.create_client_connection(protocol_config.as_ref()).await?;
            self.current_session_id = Some(session_id);
            tracing::info!("✅ 客户端连接成功，会话ID: {}", session_id);
            Ok(())
        } else {
            Err(TransportError::config_error("protocol", "No protocol configured - use with_protocol() to specify protocol"))
        }
    }
    
    /// 🔌 客户端断开连接 - 简化API
    pub async fn disconnect(&mut self) -> Result<(), TransportError> {
        if let Some(session_id) = self.current_session_id.take() {
            self.inner.close_session(session_id).await?;
            tracing::info!("✅ 客户端连接已断开，会话ID: {}", session_id);
            Ok(())
        } else {
            tracing::warn!("客户端未连接，无需断开");
            Ok(())
        }
    }
    
    /// 📨 客户端发送消息 - 简化API，无需session_id
    pub async fn send(&self, packet: crate::packet::Packet) -> Result<(), TransportError> {
        if let Some(session_id) = self.current_session_id {
            self.inner.send_to_session(session_id, packet).await
        } else {
            Err(TransportError::connection_error("Not connected - call connect() first", false))
        }
    }
    
    /// 📊 检查连接状态
    pub fn is_connected(&self) -> bool {
        self.current_session_id.is_some()
    }
    
    /// 🔍 获取当前会话ID (仅用于调试)
    pub fn current_session(&self) -> Option<SessionId> {
        self.current_session_id
    }
    
    /// 批量连接
    pub async fn connect_multiple<C>(&self, configs: Vec<C>) -> Result<Vec<SessionId>, TransportError>
    where
        C: ProtocolConfig + ConnectableConfig + Clone,
    {
        let mut sessions = Vec::new();
        for config in configs {
            let session_id = self.with_protocol(config).connect().await?;
            sessions.push(session_id);
        }
        Ok(sessions)
    }
    
    /// 创建连接池
    pub async fn create_connection_pool<C>(
        &self, 
        protocol_name: String,
        config: C, 
        pool_size: usize
    ) -> Result<(), TransportError>
    where
        C: ProtocolConfig + ConnectableConfig + Clone + 'static,
    {
        let pool = ClientConnectionPool::new(self, config, pool_size).await?;
        let mut pools = self.connection_pools.write().await;
        pools.insert(protocol_name, pool);
        Ok(())
    }
    
    /// 从连接池获取连接
    pub async fn get_pooled_connection(&self, protocol_name: &str) -> Result<SessionId, TransportError> {
        let pools = self.connection_pools.read().await;
        if let Some(pool) = pools.get(protocol_name) {
            pool.get_connection().await
        } else {
            Err(TransportError::config_error("pool", format!("Connection pool not found: {}", protocol_name)))
        }
    }
    
    // 委托给内部transport的通用方法 - 高级API
    pub async fn send_to_session(&self, session_id: SessionId, packet: Packet) -> Result<(), TransportError> {
        self.inner.send_to_session(session_id, packet).await
    }
    
    pub async fn close_session(&self, session_id: SessionId) -> Result<(), TransportError> {
        self.inner.close_session(session_id).await
    }
    
    pub fn events(&self) -> EventStream {
        self.inner.events()
    }
    
    pub fn session_events(&self, session_id: SessionId) -> EventStream {
        self.inner.session_events(session_id)
    }
    
    /// 获取连接统计
    pub async fn stats(&self) -> Result<std::collections::HashMap<SessionId, crate::command::TransportStats>, TransportError> {
        self.inner.stats().await
    }
    
    /// 获取活跃会话
    pub async fn active_sessions(&self) -> Result<Vec<SessionId>, TransportError> {
        Ok(self.inner.active_sessions().await)
    }
}

/// 客户端连接构建器 - 流式API实现
pub struct ProtocolConnectionBuilder<'t, C> {
    transport: &'t ClientTransport,
    config: C,
    connection_options: ConnectionOptions,
}

impl<'t, C> ProtocolConnectionBuilder<'t, C>
where
    C: ProtocolConfig + ConnectableConfig,
{
    pub(crate) fn new(transport: &'t ClientTransport, config: C) -> Self {
        Self { 
            transport, 
            config,
            connection_options: ConnectionOptions::default(),
        }
    }
    
    /// 设置连接超时
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connection_options.timeout = Some(timeout);
        self
    }
    
    /// 设置重试次数
    pub fn with_retry(mut self, max_retries: usize) -> Self {
        self.connection_options.max_retries = max_retries;
        self
    }
    
    /// 设置连接优先级
    pub fn with_priority(mut self, priority: ConnectionPriority) -> Self {
        self.connection_options.priority = priority;
        self
    }
    
    /// 🎯 执行连接 - 核心方法
    pub async fn connect(self) -> Result<SessionId, TransportError> {
        // 配置验证
        self.config.validate()
            .map_err(|e| TransportError::config_error("protocol", format!("Config validation failed: {:?}", e)))?;
        
        // 应用超时选项
        if let Some(timeout) = self.connection_options.timeout {
            tokio::time::timeout(timeout, self.connect_inner()).await
                .map_err(|_| TransportError::connection_error("Connection timeout", true))?
        } else {
            self.connect_inner().await
        }
    }
    
    async fn connect_inner(self) -> Result<SessionId, TransportError> {
        let mut last_error = None;
        let max_retries = if self.connection_options.max_retries > 0 {
            self.connection_options.max_retries
        } else {
            self.transport.retry_config.max_retries
        };
        
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = self.calculate_retry_delay(attempt);
                tracing::debug!("连接重试 {}/{}, 延迟: {:?}", attempt, max_retries, delay);
                tokio::time::sleep(delay).await;
            }
            
            match self.config.connect(&self.transport.inner).await {
                Ok(session_id) => {
                    tracing::info!("连接建立成功: {:?}", session_id);
                    return Ok(session_id);
                },
                Err(e) => {
                    tracing::warn!("连接尝试 {} 失败: {:?}", attempt + 1, e);
                    last_error = Some(e);
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| 
            TransportError::connection_error("All retry attempts failed", true)
        ))
    }
    
    fn calculate_retry_delay(&self, attempt: usize) -> Duration {
        let base_delay = self.transport.retry_config.initial_delay;
        let multiplier = self.transport.retry_config.backoff_multiplier;
        let max_delay = self.transport.retry_config.max_delay;
        
        let calculated_delay = Duration::from_millis(
            (base_delay.as_millis() as f64 * multiplier.powi(attempt as i32 - 1)) as u64
        );
        
        std::cmp::min(calculated_delay, max_delay)
    }
}

/// 客户端连接池
pub struct ClientConnectionPool {
    // 简化实现，实际可以更复杂
    sessions: Vec<SessionId>,
    current_index: std::sync::atomic::AtomicUsize,
}

impl ClientConnectionPool {
    async fn new<C>(
        transport: &ClientTransport, 
        config: C, 
        pool_size: usize
    ) -> Result<Self, TransportError>
    where
        C: ProtocolConfig + ConnectableConfig + Clone,
    {
        let mut sessions = Vec::with_capacity(pool_size);
        
        for _ in 0..pool_size {
            let session_id = transport.with_protocol(config.clone()).connect().await?;
            sessions.push(session_id);
        }
        
        Ok(Self {
            sessions,
            current_index: std::sync::atomic::AtomicUsize::new(0),
        })
    }
    
    async fn get_connection(&self) -> Result<SessionId, TransportError> {
        if self.sessions.is_empty() {
            return Err(TransportError::connection_error("Connection pool is empty", true));
        }
        
        let index = self.current_index.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % self.sessions.len();
        Ok(self.sessions[index])
    }
} 