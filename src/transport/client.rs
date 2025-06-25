/// 客户端传输层模块
/// 
/// 提供专门针对客户端连接的传输层API

use std::time::Duration;
use std::sync::Arc;
use tokio::sync::RwLock;
use bytes::Bytes;

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
    inner: Arc<Transport>,
    retry_config: RetryConfig,
    // 客户端协议配置
    protocol_config: Option<Box<dyn DynClientConfig>>,
    // 🎯 当前连接的会话ID - 使用 Arc<RwLock> 以便修改
    current_session_id: Arc<RwLock<Option<SessionId>>>,
    event_sender: tokio::sync::broadcast::Sender<crate::event::ClientEvent>,
    // 🎯 消息ID计数器 - 用于自动生成消息ID
    message_id_counter: std::sync::atomic::AtomicU32,
}

impl TransportClient {
    pub(crate) fn new(
        transport: Transport,
        retry_config: RetryConfig,
        protocol_config: Option<Box<dyn DynClientConfig>>,
    ) -> Self {
        Self {
            inner: Arc::new(transport),
            retry_config,
            protocol_config,
            current_session_id: Arc::new(RwLock::new(None)),
            event_sender: tokio::sync::broadcast::channel(16).0,
            message_id_counter: std::sync::atomic::AtomicU32::new(1),
        }
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
        
        // ⭐️ 启动事件转发任务，将Transport事件转换为ClientEvent
        self.start_event_forwarding().await?;
        
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
                        match self.inner.connect_with_config(tcp_config.clone()).await {
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
                        match self.inner.connect_with_config(ws_config.clone()).await {
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
                        match self.inner.connect_with_config(quic_config.clone()).await {
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
    pub async fn disconnect(&self) -> Result<(), TransportError> {
        // 检查是否已连接
        let mut current_session = self.current_session_id.write().await;
        if let Some(session_id) = current_session.take() {
            drop(current_session);
            
            tracing::info!("TransportClient 断开连接");
            
            // 使用 Transport 的统一关闭方法
            self.inner.close_session(session_id).await?;
            
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🔌 强制断开连接
    pub async fn force_disconnect(&self) -> Result<(), TransportError> {
        // 检查是否已连接
        let mut current_session = self.current_session_id.write().await;
        if let Some(session_id) = current_session.take() {
            drop(current_session);
            
            tracing::info!("TransportClient 强制断开连接");
            
            // 使用 Transport 的强制关闭方法
            self.inner.force_close_session(session_id).await?;
            
            Ok(())
        } else {
            Err(TransportError::connection_error("Not connected", false))
        }
    }
    
    /// 🚀 发送字节数据 - 统一API返回TransportResult
    pub async fn send(&self, data: &[u8]) -> Result<crate::event::TransportResult, TransportError> {
        if !self.is_connected().await {
            return Err(TransportError::connection_error("Not connected - call connect() first", false));
        }
        
        let message_id = self.message_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = crate::packet::Packet::one_way(message_id, data.to_vec());
        
        tracing::debug!("TransportClient 发送数据: {} bytes (ID: {})", data.len(), message_id);
        
        match self.inner.send(packet).await {
            Ok(()) => {
                // 发送成功，返回TransportResult
                Ok(crate::event::TransportResult::new_sent(None, message_id))
            }
            Err(e) => Err(e),
        }
    }
    
    /// 🔄 发送字节请求并等待响应 - 统一API返回TransportResult
    pub async fn request(&self, data: &[u8]) -> Result<crate::event::TransportResult, TransportError> {
        if !self.is_connected().await {
            return Err(TransportError::connection_error("Not connected - call connect() first", false));
        }
        
        let message_id = self.message_id_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let packet = crate::packet::Packet::request(message_id, data.to_vec());
        
        tracing::debug!("TransportClient 发送请求: {} bytes (ID: {})", data.len(), message_id);
        
        match self.inner.request(packet).await {
            Ok(response_packet) => {
                tracing::debug!("TransportClient 收到响应: {} bytes (ID: {})", response_packet.payload.len(), response_packet.header.message_id);
                // 请求成功，返回包含响应数据的TransportResult
                Ok(crate::event::TransportResult::new_completed(None, message_id, response_packet.payload.clone()))
            }
            Err(e) => {
                // 判断是否为超时错误
                if e.to_string().contains("timeout") || e.to_string().contains("Timeout") {
                    Ok(crate::event::TransportResult::new_timeout(None, message_id))
                } else {
                    Err(e)
                }
            }
        }
    }
    
    /// 📊 检查连接状态
    pub async fn is_connected(&self) -> bool {
        self.inner.is_connected().await
    }

    /// 获取连接状态信息
    pub async fn connection_info(&self) -> Option<crate::command::ConnectionInfo> {
        // TODO: 实现连接信息获取
        None
    }

    /// 获取当前会话ID
    pub async fn current_session_id(&self) -> Option<SessionId> {
        self.inner.current_session_id().await
    }
    
    /// 获取客户端事件流 - 返回当前连接的事件流（隐藏会话ID）
    pub async fn events(&self) -> Result<crate::stream::ClientEventStream, TransportError> {
        use crate::stream::StreamFactory;
        
        // 检查是否已连接
        if !self.is_connected().await {
            return Err(TransportError::connection_error("Not connected - call connect() first", false));
        }
        
        // 🔧 修复：直接使用Transport的事件流，不再依赖会话ID
        if let Some(event_receiver) = self.inner.get_event_stream().await {
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
        self.inner.current_session_id().await
    }
    
    /// 获取客户端连接统计
    /// TODO: Transport 需要实现统计功能
    pub async fn stats(&self) -> Result<crate::command::TransportStats, TransportError> {
        // 暂时返回错误，等待 Transport 实现统计
        Err(TransportError::connection_error("Stats not implemented for Transport yet", false))
    }



    /// 业务层订阅 ClientEvent
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<crate::event::ClientEvent> {
        self.event_sender.subscribe()
    }

    /// ⭐️ 启动事件转发任务
    async fn start_event_forwarding(&self) -> Result<(), TransportError> {
        // 获取Transport的事件流
        if let Some(mut transport_events) = self.inner.get_event_stream().await {
            let client_event_sender = self.event_sender.clone();
            let transport_for_response = self.inner.clone();
            
            // 启动转发任务
            tokio::spawn(async move {
                tracing::debug!("🔄 TransportClient 事件转发任务启动");
                
                while let Ok(transport_event) = transport_events.recv().await {
                    tracing::debug!("📥 TransportClient 收到Transport事件");
                    
                    // 🎯 特殊处理 MessageReceived 中的 Request 包
                    match &transport_event {
                        crate::event::TransportEvent::MessageReceived(packet) 
                            if packet.header.packet_type == crate::packet::PacketType::Request => {
                            
                            // 为 Request 包创建带真实响应功能的 TransportContext
                            let transport = transport_for_response.clone();
                            let message_id = packet.header.message_id;
                            
                                                        let context = crate::event::TransportContext::new_request(
                                None,
                                message_id,
                                packet.header.biz_type,
                                if packet.ext_header.is_empty() { None } else { Some(packet.ext_header.clone()) },
                                packet.payload.clone(),
                                Arc::new(move |response_data: Vec<u8>| {
                                    let transport = transport.clone();
                                    tokio::spawn(async move {
                                        // 🎯 创建响应包
                                        let response_packet = crate::packet::Packet {
                                            header: crate::packet::FixedHeader {
                                                version: 1,
                                                compression: crate::packet::CompressionType::None,
                                                packet_type: crate::packet::PacketType::Response,
                                                biz_type: 0,
                                                message_id,
                                                ext_header_len: 0,
                                                payload_len: response_data.len() as u32,
                                                reserved: crate::packet::ReservedFlags::new(),
                                            },
                                            ext_header: Vec::new(),
                                            payload: response_data,
                                        };
                                        
                                        if let Err(e) = transport.send(response_packet).await {
                                            tracing::error!("❌ TransportClient 发送响应失败: {}", e);
                                        }
                                    });
                                })
                            );
                            
                            let client_event = crate::event::ClientEvent::MessageReceived(context);
                            tracing::debug!("📤 TransportClient 转发ClientEvent (Request): {:?}", client_event);
                            
                            if let Err(e) = client_event_sender.send(client_event) {
                                tracing::warn!("⚠️ TransportClient 事件转发失败: {:?}", e);
                            }
                        }
                        _ => {
                            // 其他事件使用标准转换
                            if let Some(client_event) = crate::event::ClientEvent::from_transport_event(transport_event) {
                                tracing::debug!("📤 TransportClient 转发ClientEvent: {:?}", client_event);
                                
                                if let Err(e) = client_event_sender.send(client_event) {
                                    tracing::warn!("⚠️ TransportClient 事件转发失败: {:?}", e);
                                }
                            } else {
                                tracing::debug!("🚫 TransportClient 跳过不支持的事件");
                            }
                        }
                    }
                }
                
                tracing::debug!("📡 TransportClient 事件转发任务结束");
            });
            
            tracing::debug!("✅ TransportClient 事件转发任务已启动");
            Ok(())
        } else {
            Err(TransportError::connection_error("Connection does not support event streams", false))
        }
    }

    /// 🚀 发送请求并等待响应（带选项）
    pub async fn request_with_options(&self, data: Bytes, options: super::TransportOptions) -> Result<Bytes, TransportError> {
        self.inner.request_with_options(data, options).await
    }

    /// 🚀 发送单向消息（带选项）
    pub async fn send_with_options(&self, data: Bytes, options: super::TransportOptions) -> Result<(), TransportError> {
        self.inner.send_with_options(data, options).await
    }
}

// 简化完成 - 符合用户要求的唯一连接方式 