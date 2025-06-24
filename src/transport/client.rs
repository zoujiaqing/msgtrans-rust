/// 客户端传输层模块
/// 
/// 提供专门针对客户端连接的传输层API

use std::time::Duration;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    SessionId,
    error::TransportError,
    transport::config::TransportConfig,
    protocol::adapter::{DynProtocolConfig, DynClientConfig},
};

// 内部使用新的 Transport 结构体
use super::transport::Transport;

/// 连接配置 trait - 本地定义
pub trait ConnectableConfig {
    async fn connect(&self, transport: &mut Transport) -> Result<SessionId, TransportError>;
    fn validate(&self) -> Result<(), TransportError>;
    fn protocol_name(&self) -> &'static str;
    fn as_any(&self) -> &dyn std::any::Any;
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
    protocol_config: Option<Box<dyn DynClientConfig>>,
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
    pub fn with_protocol<T: DynClientConfig>(mut self, config: T) -> Self {
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
    pub async fn build(self) -> Result<TransportClient, TransportError> {
        // 创建底层 Transport
        let transport = Transport::new(self.transport_config).await?;
        
        Ok(TransportClient::new(
            transport,
            self.retry_config,
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
    inner: Arc<tokio::sync::Mutex<Transport>>,
    retry_config: RetryConfig,
    // 客户端协议配置
    protocol_config: Option<Box<dyn DynClientConfig>>,
    // 🎯 当前连接的会话ID - 使用 Arc<RwLock> 以便修改
    current_session_id: Arc<RwLock<Option<SessionId>>>,
}

impl TransportClient {
    pub(crate) fn new(
        transport: Transport,
        retry_config: RetryConfig,
        protocol_config: Option<Box<dyn DynClientConfig>>,
    ) -> Self {
        Self {
            inner: Arc::new(tokio::sync::Mutex::new(transport)),
            retry_config,
            protocol_config,
            current_session_id: Arc::new(RwLock::new(None)),
        }
    }
    
    /// 启动内部事件处理任务，自动处理响应包
    fn start_internal_event_handler(&self) {
        let transport = self.inner.clone();
        
        tokio::spawn(async move {
            // 等待连接建立完成
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            
            // 获取连接适配器并尝试获取事件接收器
            let transport_guard = transport.lock().await;
            if let Some(connection_adapter) = transport_guard.connection_adapter() {
                let conn = connection_adapter.lock().await;
                if let Some(mut event_receiver) = conn.event_stream() {
                    drop(conn); // 释放连接锁
                    drop(transport_guard); // 释放transport锁
                    tracing::debug!("🔄 TransportClient 内部事件处理器启动");
                    
                    while let Ok(event) = event_receiver.recv().await {
                        match event {
                            crate::event::TransportEvent::MessageReceived { packet, .. } => {
                                // 添加详细的包类型调试信息
                                tracing::debug!("📦 收到数据包: message_id={}, packet_type={:?} (u8={})", 
                                    packet.message_id(), packet.packet_type(), u8::from(packet.packet_type()));
                                
                                // 重新获取transport来处理包
                                let transport_guard = transport.lock().await;
                                // 自动处理接收到的包，如果是响应包则会被消费
                                if let Some(unhandled_packet) = transport_guard.handle_incoming_packet(packet) {
                                    // 如果不是响应包，则继续保留在事件流中
                                    // 这里可以选择忽略或记录日志
                                    tracing::debug!("📦 收到非响应包: message_id={}, packet_type={:?}", 
                                        unhandled_packet.message_id(), unhandled_packet.packet_type());
                                } else {
                                    tracing::debug!("✅ 响应包已被消费: packet_type={:?}", crate::packet::PacketType::Response);
                                }
                                drop(transport_guard);
                            }
                            crate::event::TransportEvent::ConnectionClosed { .. } => {
                                tracing::debug!("🔗 TransportClient 检测到连接关闭，停止内部事件处理");
                                break;
                            }
                            _ => {
                                // 其他事件不处理
                            }
                        }
                    }
                    
                    tracing::debug!("🔄 TransportClient 内部事件处理器结束");
                } else {
                    tracing::debug!("🔄 TransportClient 连接不支持事件流，跳过内部事件处理");
                }
            } else {
                tracing::debug!("🔄 TransportClient 无连接适配器，跳过内部事件处理");
            }
        });
    }
    
    /// 🔌 使用构建时指定的协议配置进行连接 - 框架唯一连接方式
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        // 检查是否有协议配置并克隆以避免借用冲突
        let protocol_config = self.protocol_config.as_ref()
            .ok_or_else(|| TransportError::config_error("protocol", 
                "No protocol config specified. Use TransportClientBuilder::with_protocol() when building."))?
            .clone_client_dyn();

        // 验证协议配置
        protocol_config.validate_dyn().map_err(|e| TransportError::config_error("protocol", format!("Config validation failed: {:?}", e)))?;
        
        // 使用存储的协议配置连接
        let session_id = self.connect_with_stored_config(&protocol_config).await?;
        
        // 更新当前会话ID (内部使用)
        let mut current_session = self.current_session_id.write().await;
        *current_session = Some(session_id);
        drop(current_session);
        
        // RPC 响应包现在直接在 Transport::request_with_timeout 中处理
        
        tracing::info!("✅ TransportClient 连接成功");
        Ok(())
    }

    /// 🔧 内部方法：使用存储的协议配置连接
    async fn connect_with_stored_config(&mut self, protocol_config: &Box<dyn DynClientConfig>) -> Result<SessionId, TransportError> {
        let mut last_error = None;
        let max_retries = self.retry_config.max_retries;
        
        for attempt in 0..=max_retries {
            if attempt > 0 {
                let delay = self.calculate_retry_delay(attempt);
                tracing::debug!("连接重试 {}/{}, 延迟: {:?}", attempt, max_retries, delay);
                tokio::time::sleep(delay).await;
            }
            
            // 根据协议类型进行连接
            match protocol_config.protocol_name() {
                "tcp" => {
                    if let Some(tcp_config) = protocol_config.as_any().downcast_ref::<crate::protocol::TcpClientConfig>() {
                        match self.inner.lock().await.connect_with_config(tcp_config).await {
                            Ok(session_id) => return Ok(session_id),
                            Err(e) => {
                                last_error = Some(e);
                                tracing::warn!("TCP连接失败 (尝试 {}): {:?}", attempt + 1, last_error);
                            }
                        }
                    } else {
                        return Err(TransportError::config_error("protocol", "Invalid TCP config"));
                    }
                }
                "websocket" => {
                    if let Some(ws_config) = protocol_config.as_any().downcast_ref::<crate::protocol::WebSocketClientConfig>() {
                        match self.inner.lock().await.connect_with_config(ws_config).await {
                            Ok(session_id) => return Ok(session_id),
                            Err(e) => {
                                last_error = Some(e);
                                tracing::warn!("WebSocket连接失败 (尝试 {}): {:?}", attempt + 1, last_error);
                            }
                        }
                    } else {
                        return Err(TransportError::config_error("protocol", "Invalid WebSocket config"));
                    }
                }
                "quic" => {
                    if let Some(quic_config) = protocol_config.as_any().downcast_ref::<crate::protocol::QuicClientConfig>() {
                        match self.inner.lock().await.connect_with_config(quic_config).await {
                            Ok(session_id) => return Ok(session_id),
                            Err(e) => {
                                last_error = Some(e);
                                tracing::warn!("QUIC连接失败 (尝试 {}): {:?}", attempt + 1, last_error);
                            }
                        }
                    } else {
                        return Err(TransportError::config_error("protocol", "Invalid QUIC config"));
                    }
                }
                protocol_name => {
                    return Err(TransportError::config_error("protocol", format!("Unsupported protocol: {}", protocol_name)));
                }
            }
        }
        
        // 所有重试都失败了
        Err(last_error.unwrap_or_else(|| TransportError::connection_error("Connection failed after all retries", true)))
    }

    fn calculate_retry_delay(&self, attempt: usize) -> std::time::Duration {
        let delay = self.retry_config.initial_delay.as_secs_f64() 
            * self.retry_config.backoff_multiplier.powi(attempt as i32);
        let delay = delay.min(self.retry_config.max_delay.as_secs_f64());
        std::time::Duration::from_secs_f64(delay)
    }
    
    /// 📡 断开连接（优雅关闭）
    pub async fn disconnect(&mut self) -> Result<(), TransportError> {
        // 检查是否已连接
        let mut current_session = self.current_session_id.write().await;
        if let Some(session_id) = current_session.take() {
            drop(current_session);
            
            tracing::info!("TransportClient 断开连接");
            
            // 使用 Transport 的统一关闭方法
            self.inner.lock().await.close_session(session_id).await?;
            
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🔌 强制断开连接
    pub async fn force_disconnect(&mut self) -> Result<(), TransportError> {
        // 检查是否已连接
        let mut current_session = self.current_session_id.write().await;
        if let Some(session_id) = current_session.take() {
            drop(current_session);
            
            tracing::info!("TransportClient 强制断开连接");
            
            // 使用 Transport 的强制关闭方法
            self.inner.lock().await.force_close_session(session_id).await?;
            
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🚀 发送数据包 - 客户端核心方法
    pub async fn send(&self, packet: crate::packet::Packet) -> Result<(), TransportError> {
        if self.is_connected().await {
            tracing::debug!("TransportClient 发送数据包到当前连接");
            self.inner.lock().await.send(packet).await
        } else {
            Err(TransportError::connection_error("Not connected - call connect() first", false))
        }
    }
    
    /// 🚀 发送请求并等待响应 - 默认10秒超时
    pub async fn request(&self, packet: crate::packet::Packet) -> Result<crate::packet::Packet, TransportError> {
        if self.is_connected().await {
            tracing::debug!("TransportClient 发送 RPC 请求到当前连接");
            self.inner.lock().await.request(packet).await
        } else {
            Err(TransportError::connection_error("Not connected - call connect() first", false))
        }
    }
    
    /// 🚀 发送请求并等待响应 - 自定义超时
    pub async fn request_with_timeout(&self, packet: crate::packet::Packet, timeout: std::time::Duration) -> Result<crate::packet::Packet, TransportError> {
        if self.is_connected().await {
            tracing::debug!("TransportClient 发送 RPC 请求到当前连接（超时: {:?}）", timeout);
            self.inner.lock().await.request_with_timeout(packet, timeout).await
        } else {
            Err(TransportError::connection_error("Not connected - call connect() first", false))
        }
    }
    
    /// 📊 检查连接状态
    pub async fn is_connected(&self) -> bool {
        self.inner.lock().await.is_connected()
    }
    
    /// 获取客户端事件流 - 返回当前连接的事件流（隐藏会话ID）
    pub async fn events(&self) -> Result<crate::stream::ClientEventStream, TransportError> {
        use crate::stream::StreamFactory;
        
        // 检查是否已连接
        if !self.is_connected().await {
            return Err(TransportError::connection_error("Not connected - call connect() first", false));
        }
        
        // 🔧 修复：直接使用Transport的事件流，不再依赖会话ID
        if let Some(event_receiver) = self.inner.lock().await.get_event_stream().await {
            tracing::debug!("✅ TransportClient 获取到连接适配器的事件流");
            tracing::debug!("📡 TransportClient 客户端事件流创建完成");
            return Ok(StreamFactory::client_event_stream(event_receiver));
        } else {
            // 如果无法获取事件流，返回错误
            return Err(TransportError::connection_error("Connection does not support event streams", false));
        }
    }
    
    /// 🔍 内部方法：获取当前会话ID (仅用于内部调试)
    async fn current_session(&self) -> Option<SessionId> {
        self.inner.lock().await.current_session_id()
    }
    
    /// 获取客户端连接统计
    /// TODO: Transport 需要实现统计功能
    pub async fn stats(&self) -> Result<crate::command::TransportStats, TransportError> {
        // 暂时返回错误，等待 Transport 实现统计
        Err(TransportError::connection_error("Stats not implemented for Transport yet", false))
    }
}

// 简化完成 - 符合用户要求的唯一连接方式 