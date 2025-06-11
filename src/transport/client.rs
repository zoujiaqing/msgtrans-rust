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
    transport::{
        config::TransportConfig,
        expert_config::ExpertConfig,
    },
    protocol::{ProtocolConfig, adapter::DynProtocolConfig},
    stream::EventStream,
    packet::Packet,
    command::TransportStats,
};

// 内部使用新的 Transport 结构体
use super::transport::Transport;

/// 连接配置 trait - 本地定义
pub trait ConnectableConfig {
    async fn connect(&self, transport: &Transport) -> Result<SessionId, TransportError>;
    fn validate(&self) -> Result<(), TransportError>;
}

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
            health_check_interval: Duration::from_secs(30),
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
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
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

/// 断路器配置
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
#[derive(Debug, Clone)]
pub enum ConnectionPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// 客户端传输层构建器
pub struct TransportClientBuilder {
    connect_timeout: Duration,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    load_balancer: Option<LoadBalancerConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    connection_monitoring: bool,
    transport_config: TransportConfig,
    /// 协议配置存储 - 客户端只支持一个协议连接
    protocol_config: Option<Box<dyn DynProtocolConfig>>,
}

impl TransportClientBuilder {
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(30),
            pool_config: ConnectionPoolConfig::default(),
            retry_config: RetryConfig::default(),
            load_balancer: None,
            circuit_breaker: None,
            connection_monitoring: false,
            transport_config: TransportConfig::default(),
            protocol_config: None,
        }
    }
    
    /// 设置协议配置 - 客户端特定
    pub fn with_protocol<T: DynProtocolConfig>(mut self, config: T) -> Self {
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

    /// 客户端专用：断路器
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

    /// 构建客户端传输层 - 返回 TransportClient
    pub async fn build(mut self) -> Result<TransportClient, TransportError> {
        // 创建底层 Transport
        let transport = Transport::new(self.transport_config).await?;
        
        Ok(TransportClient::new(
            transport,
            self.pool_config,
            self.retry_config,
            self.load_balancer,
            self.circuit_breaker,
            self.protocol_config,
        ))
    }
}

impl Default for TransportClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 🎯 传输层客户端 - 使用 Transport 进行单连接管理
pub struct TransportClient {
    inner: Transport,
    pool_config: ConnectionPoolConfig,
    retry_config: RetryConfig,
    load_balancer: Option<LoadBalancerConfig>,
    circuit_breaker: Option<CircuitBreakerConfig>,
    // 客户端协议配置
    protocol_config: Option<Box<dyn DynProtocolConfig>>,
    // 客户端专用状态
    connection_pools: Arc<RwLock<HashMap<String, ClientConnectionPool>>>,
    // 🎯 当前连接的会话ID - 使用 Arc<RwLock> 以便修改
    current_session_id: Arc<RwLock<Option<SessionId>>>,
}

impl TransportClient {
    pub(crate) fn new(
        transport: Transport,
        pool_config: ConnectionPoolConfig,
        retry_config: RetryConfig,
        load_balancer: Option<LoadBalancerConfig>,
        circuit_breaker: Option<CircuitBreakerConfig>,
        protocol_config: Option<Box<dyn DynProtocolConfig>>,
    ) -> Self {
        Self {
            inner: transport,
            pool_config,
            retry_config,
            load_balancer,
            circuit_breaker,
            protocol_config,
            connection_pools: Arc::new(RwLock::new(HashMap::new())),
            current_session_id: Arc::new(RwLock::new(None)),
        }
    }
    
    /// 🔌 使用指定协议配置进行连接
    pub fn with_protocol<C>(&self, config: C) -> ProtocolConnectionBuilder<'_, C>
    where
        C: ProtocolConfig + ConnectableConfig,
    {
        ProtocolConnectionBuilder::new(self, config)
    }
    
    /// 🔌 使用默认协议连接 (必须在构建时指定)
    pub async fn connect(&self) -> Result<(), TransportError> {
        // 注意：这里的实现需要协议配置实现ConnectableConfig trait
        // 对于Builder模式，我们建议使用 with_protocol().connect() 方式
        Err(TransportError::config_error("protocol", 
            "Default protocol connection not supported yet. Use with_protocol(config).connect() instead."))
    }
    
    /// 📡 断开连接
    pub async fn disconnect(&mut self) -> Result<(), TransportError> {
        // 清除当前会话ID
        let mut current_session = self.current_session_id.write().await;
        if let Some(session_id) = current_session.take() {
            drop(current_session);
            
            tracing::info!("TransportClient 断开连接 (会话: {})", session_id);
            self.inner.disconnect().await?;
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🚀 发送数据包 - 客户端核心方法
    pub async fn send(&self, packet: crate::packet::Packet) -> Result<(), TransportError> {
        if self.is_connected().await {
            tracing::debug!("TransportClient 发送数据包到当前连接");
            self.inner.send(packet).await
        } else {
            Err(TransportError::connection_error("Not connected - call connect() first", false))
        }
    }
    
    /// 📊 检查连接状态
    pub async fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }
    
    /// 🔍 获取当前会话ID (仅用于调试)
    pub async fn current_session(&self) -> Option<SessionId> {
        self.inner.current_session_id()
    }
    
    /// 获取客户端事件流 - 只返回当前连接的事件 
    /// TODO: Transport 需要实现事件流
    pub async fn events(&self) -> Result<EventStream, TransportError> {
        // 暂时返回错误，等待 Transport 实现事件流
        Err(TransportError::connection_error("Events not implemented for Transport yet", false))
    }
    
    /// 获取客户端连接统计
    /// TODO: Transport 需要实现统计功能
    pub async fn stats(&self) -> Result<crate::command::TransportStats, TransportError> {
        // 暂时返回错误，等待 Transport 实现统计
        Err(TransportError::connection_error("Stats not implemented for Transport yet", false))
    }

    // 高级功能保留
    
    /// 批量连接 - 高级功能
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
    
    /// 创建连接池 - 高级功能
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
    
    /// 从连接池获取连接 - 高级功能
    pub async fn get_pooled_connection(&self, protocol_name: &str) -> Result<SessionId, TransportError> {
        let pools = self.connection_pools.read().await;
        if let Some(pool) = pools.get(protocol_name) {
            pool.get_connection().await
        } else {
            Err(TransportError::config_error("pool", format!("Connection pool not found: {}", protocol_name)))
        }
    }
}

/// 客户端连接构建器 - 流式API实现
pub struct ProtocolConnectionBuilder<'t, C> {
    transport: &'t TransportClient,
    config: C,
    connection_options: ConnectionOptions,
}

impl<'t, C> ProtocolConnectionBuilder<'t, C>
where
    C: ProtocolConfig + ConnectableConfig,
{
    pub(crate) fn new(transport: &'t TransportClient, config: C) -> Self {
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
        ConnectableConfig::validate(&self.config)
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
            
            // 🎯 使用新的连接逻辑，通过 trait 对象调用
            match self.config.connect(&self.transport.inner).await {
                Ok(session_id) => {
                    tracing::info!("连接建立成功: {:?}", session_id);
                    
                    // 🎯 更新客户端的当前会话ID
                    let mut current_session = self.transport.current_session_id.write().await;
                    *current_session = Some(session_id);  
                    drop(current_session);
                    
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
        transport: &TransportClient, 
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