/// 统一配置系统
/// 
/// 简化配置接口，消除重复定义，提供清晰的配置层次结构

use async_trait::async_trait;
use serde::{Serialize, Deserialize};
use std::time::Duration;
use std::collections::HashMap;
use crate::{
    error::TransportError,
    connection::{Connection, Server},
};

/// 配置错误
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid value for field '{field}': {value} - {reason}. Suggestion: {suggestion}")]
    InvalidValue {
        field: String,
        value: String,
        reason: String,
        suggestion: String,
    },
    
    #[error("Missing required field: {field}. {suggestion}")]
    MissingField {
        field: String,
        suggestion: String,
    },
    
    #[error("Invalid address: {address} - {reason}")]
    InvalidAddress {
        address: String,
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("Configuration conflict: {message}")]
    Conflict { message: String },
}

/// 统一的协议配置接口
/// 
/// 这是唯一的配置基础接口，所有协议配置都应该实现此接口
pub trait ProtocolConfig: Send + Sync + Clone + std::fmt::Debug {
    /// 协议名称
    fn protocol_name(&self) -> &'static str;
    
    /// 验证配置
    fn validate(&self) -> Result<(), ConfigError>;
    
    /// 获取配置的字符串表示（用于调试）
    fn config_summary(&self) -> String {
        format!("{} config", self.protocol_name())
    }
}

/// 客户端配置接口
/// 
/// 用于创建客户端连接的配置
#[async_trait]
pub trait ClientConfig: ProtocolConfig {
    /// 创建客户端连接
    async fn create_connection(&self) -> Result<Box<dyn Connection>, TransportError>;
    
    /// 获取目标地址信息
    fn target_info(&self) -> String;
}

/// 服务端配置接口
/// 
/// 用于创建服务器的配置
#[async_trait]
pub trait ServerConfig: ProtocolConfig {
    /// 创建服务器
    async fn create_server(&self) -> Result<Box<dyn Server>, TransportError>;
    
    /// 获取绑定地址
    fn bind_address(&self) -> std::net::SocketAddr;
}

/// 重试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// 最大重试次数
    pub max_retries: u32,
    /// 重试间隔
    pub retry_interval: Duration,
    /// 指数退避系数
    pub backoff_multiplier: f64,
    /// 最大重试间隔
    pub max_retry_interval: Duration,
    /// 是否启用抖动
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_interval: Duration::from_millis(500),
            backoff_multiplier: 2.0,
            max_retry_interval: Duration::from_secs(30),
            jitter: true,
        }
    }
}

/// TCP 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpClientConfig {
    /// 目标服务器地址
    pub target_address: std::net::SocketAddr,
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// TCP_NODELAY选项
    pub nodelay: bool,
    /// keepalive时间
    pub keepalive: Option<Duration>,
    /// 读缓冲区大小
    pub read_buffer_size: usize,
    /// 写缓冲区大小
    pub write_buffer_size: usize,
    /// 重连配置
    pub retry_config: RetryConfig,
}

impl Default for TcpClientConfig {
    fn default() -> Self {
        Self {
            target_address: "127.0.0.1:80".parse().unwrap(),
            connect_timeout: Duration::from_secs(10),
            nodelay: true,
            keepalive: Some(Duration::from_secs(60)),
            read_buffer_size: 8192,
            write_buffer_size: 8192,
            retry_config: RetryConfig::default(),
        }
    }
}

impl ProtocolConfig for TcpClientConfig {
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
    
    fn validate(&self) -> Result<(), ConfigError> {
        if self.read_buffer_size == 0 {
            return Err(ConfigError::InvalidValue {
                field: "read_buffer_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value like 8192".to_string(),
            });
        }
        
        if self.write_buffer_size == 0 {
            return Err(ConfigError::InvalidValue {
                field: "write_buffer_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value like 8192".to_string(),
            });
        }
        
        Ok(())
    }
    
    fn config_summary(&self) -> String {
        format!("TCP client -> {}", self.target_address)
    }
}

#[async_trait]
impl ClientConfig for TcpClientConfig {
    async fn create_connection(&self) -> Result<Box<dyn Connection>, TransportError> {
        let adapter = crate::adapters::tcp::TcpAdapter::connect(self.target_address, self.clone())
            .await
            .map_err(|e| TransportError::connection_error(format!("TCP connect failed: {:?}", e), true))?;
            
        Ok(Box::new(crate::connection::TcpConnection::new(adapter)))
    }
    
    fn target_info(&self) -> String {
        self.target_address.to_string()
    }
}

impl TcpClientConfig {
    /// 创建新的TCP客户端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置目标服务器地址
    pub fn target<A: Into<std::net::SocketAddr>>(mut self, addr: A) -> Self {
        self.target_address = addr.into();
        self
    }
    
    /// 设置连接超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    
    /// 设置TCP_NODELAY选项
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }
    
    /// 设置keepalive时间
    pub fn keepalive(mut self, keepalive: Option<Duration>) -> Self {
        self.keepalive = keepalive;
        self
    }
    
    /// 设置缓冲区大小
    pub fn buffer_size(mut self, read_size: usize, write_size: usize) -> Self {
        self.read_buffer_size = read_size;
        self.write_buffer_size = write_size;
        self
    }
    
    /// 设置重连配置
    pub fn retry(mut self, config: RetryConfig) -> Self {
        self.retry_config = config;
        self
    }
    
    /// 构建并验证配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
}

/// TCP 服务端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpServerConfig {
    /// 绑定地址
    pub bind_address: std::net::SocketAddr,
    /// 最大连接数
    pub max_connections: usize,
    /// TCP_NODELAY选项
    pub nodelay: bool,
    /// keepalive时间
    pub keepalive: Option<Duration>,
    /// 读缓冲区大小
    pub read_buffer_size: usize,
    /// 写缓冲区大小
    pub write_buffer_size: usize,
}

impl Default for TcpServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            max_connections: 1000,
            nodelay: true,
            keepalive: Some(Duration::from_secs(60)),
            read_buffer_size: 8192,
            write_buffer_size: 8192,
        }
    }
}

impl ProtocolConfig for TcpServerConfig {
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
    
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == 0 {
            return Err(ConfigError::InvalidValue {
                field: "max_connections".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value like 1000".to_string(),
            });
        }
        
        Ok(())
    }
    
    fn config_summary(&self) -> String {
        format!("TCP server @ {}", self.bind_address)
    }
}

#[async_trait]
impl ServerConfig for TcpServerConfig {
    async fn create_server(&self) -> Result<Box<dyn Server>, TransportError> {
        let server = crate::adapters::tcp::TcpServer::bind(self.bind_address, self.clone())
            .await
            .map_err(|e| TransportError::connection_error(format!("TCP bind failed: {:?}", e), false))?;
            
        Ok(Box::new(crate::connection::TcpServer::new(server)))
    }
    
    fn bind_address(&self) -> std::net::SocketAddr {
        self.bind_address
    }
}

impl TcpServerConfig {
    /// 创建新的TCP服务端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置绑定地址
    pub fn bind<A: Into<std::net::SocketAddr>>(mut self, addr: A) -> Self {
        self.bind_address = addr.into();
        self
    }
    
    /// 设置最大连接数
    pub fn max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// 设置TCP_NODELAY选项
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }
    
    /// 设置keepalive时间
    pub fn keepalive(mut self, keepalive: Option<Duration>) -> Self {
        self.keepalive = keepalive;
        self
    }
    
    /// 设置缓冲区大小
    pub fn buffer_size(mut self, read_size: usize, write_size: usize) -> Self {
        self.read_buffer_size = read_size;
        self.write_buffer_size = write_size;
        self
    }
    
    /// 构建并验证配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
}

/// WebSocket 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketClientConfig {
    /// 目标WebSocket URL
    pub url: String,
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// 子协议
    pub subprotocols: Vec<String>,
    /// 自定义头部
    pub headers: HashMap<String, String>,
    /// 最大帧大小
    pub max_frame_size: usize,
    /// 最大消息大小
    pub max_message_size: usize,
    /// ping间隔
    pub ping_interval: Option<Duration>,
    /// pong超时
    pub pong_timeout: Duration,
    /// 重连配置
    pub retry_config: RetryConfig,
}

impl Default for WebSocketClientConfig {
    fn default() -> Self {
        Self {
            url: "ws://127.0.0.1:8080/".to_string(),
            connect_timeout: Duration::from_secs(10),
            subprotocols: Vec::new(),
            headers: HashMap::new(),
            max_frame_size: 64 * 1024,
            max_message_size: 16 * 1024 * 1024,
            ping_interval: Some(Duration::from_secs(30)),
            pong_timeout: Duration::from_secs(10),
            retry_config: RetryConfig::default(),
        }
    }
}

impl ProtocolConfig for WebSocketClientConfig {
    fn protocol_name(&self) -> &'static str {
        "websocket"
    }
    
    fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingField {
                field: "url".to_string(),
                suggestion: "provide a valid WebSocket URL like 'ws://localhost:8080/'".to_string(),
            });
        }
        
        if !self.url.starts_with("ws://") && !self.url.starts_with("wss://") {
            return Err(ConfigError::InvalidValue {
                field: "url".to_string(),
                value: self.url.clone(),
                reason: "must start with 'ws://' or 'wss://'".to_string(),
                suggestion: "use a valid WebSocket URL scheme".to_string(),
            });
        }
        
        Ok(())
    }
    
    fn config_summary(&self) -> String {
        format!("WebSocket client -> {}", self.url)
    }
}

#[async_trait]
impl ClientConfig for WebSocketClientConfig {
    async fn create_connection(&self) -> Result<Box<dyn Connection>, TransportError> {
        // TODO: 实现 WebSocket 连接创建
        Err(TransportError::config_error("websocket", "WebSocket client connection not yet implemented"))
    }
    
    fn target_info(&self) -> String {
        self.url.clone()
    }
}

impl WebSocketClientConfig {
    /// 创建新的WebSocket客户端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置目标URL
    pub fn url<S: Into<String>>(mut self, url: S) -> Self {
        self.url = url.into();
        self
    }
    
    /// 设置连接超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    
    /// 添加子协议
    pub fn subprotocol<S: Into<String>>(mut self, protocol: S) -> Self {
        self.subprotocols.push(protocol.into());
        self
    }
    
    /// 添加头部
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }
    
    /// 设置帧大小限制
    pub fn frame_limits(mut self, max_frame_size: usize, max_message_size: usize) -> Self {
        self.max_frame_size = max_frame_size;
        self.max_message_size = max_message_size;
        self
    }
    
    /// 设置ping配置
    pub fn ping_config(mut self, interval: Option<Duration>, timeout: Duration) -> Self {
        self.ping_interval = interval;
        self.pong_timeout = timeout;
        self
    }
    
    /// 构建并验证配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
}

/// QUIC 客户端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicClientConfig {
    /// 目标服务器地址
    pub target_address: std::net::SocketAddr,
    /// 服务器名称（用于TLS验证）
    pub server_name: String,
    /// 连接超时时间
    pub connect_timeout: Duration,
    /// 是否验证证书
    pub verify_certificate: bool,
    /// 自定义CA证书PEM
    pub ca_cert_pem: Option<String>,
    /// 最大并发流数
    pub max_concurrent_streams: u64,
    /// 最大空闲超时
    pub max_idle_timeout: Duration,
    /// keepalive间隔
    pub keep_alive_interval: Option<Duration>,
    /// 初始RTT估值
    pub initial_rtt: Duration,
    /// 重连配置
    pub retry_config: RetryConfig,
}

impl Default for QuicClientConfig {
    fn default() -> Self {
        Self {
            target_address: "127.0.0.1:443".parse().unwrap(),
            server_name: "localhost".to_string(),
            connect_timeout: Duration::from_secs(10),
            verify_certificate: true,
            ca_cert_pem: None,
            max_concurrent_streams: 100,
            max_idle_timeout: Duration::from_secs(30),
            keep_alive_interval: Some(Duration::from_secs(15)),
            initial_rtt: Duration::from_millis(100),
            retry_config: RetryConfig::default(),
        }
    }
}

impl ProtocolConfig for QuicClientConfig {
    fn protocol_name(&self) -> &'static str {
        "quic"
    }
    
    fn validate(&self) -> Result<(), ConfigError> {
        if self.server_name.is_empty() {
            return Err(ConfigError::MissingField {
                field: "server_name".to_string(),
                suggestion: "provide a server name for TLS verification".to_string(),
            });
        }
        
        if self.max_concurrent_streams == 0 {
            return Err(ConfigError::InvalidValue {
                field: "max_concurrent_streams".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value like 100".to_string(),
            });
        }
        
        Ok(())
    }
    
    fn config_summary(&self) -> String {
        format!("QUIC client -> {}:{}", self.target_address, self.server_name)
    }
}

#[async_trait]
impl ClientConfig for QuicClientConfig {
    async fn create_connection(&self) -> Result<Box<dyn Connection>, TransportError> {
        // TODO: 实现 QUIC 连接创建
        Err(TransportError::config_error("quic", "QUIC client connection not yet implemented"))
    }
    
    fn target_info(&self) -> String {
        format!("{}:{}", self.target_address, self.server_name)
    }
}

impl QuicClientConfig {
    /// 创建新的QUIC客户端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置目标地址和服务器名称
    pub fn target<A: Into<std::net::SocketAddr>, S: Into<String>>(mut self, addr: A, server_name: S) -> Self {
        self.target_address = addr.into();
        self.server_name = server_name.into();
        self
    }
    
    /// 设置连接超时时间
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    
    /// 设置证书验证
    pub fn verify_cert(mut self, verify: bool) -> Self {
        self.verify_certificate = verify;
        self
    }
    
    /// 设置自定义CA证书
    pub fn ca_cert<S: Into<String>>(mut self, cert_pem: S) -> Self {
        self.ca_cert_pem = Some(cert_pem.into());
        self
    }
    
    /// 设置流配置
    pub fn streams(mut self, max_concurrent: u64) -> Self {
        self.max_concurrent_streams = max_concurrent;
        self
    }
    
    /// 设置超时配置
    pub fn timeouts(mut self, idle_timeout: Duration, keepalive: Option<Duration>) -> Self {
        self.max_idle_timeout = idle_timeout;
        self.keep_alive_interval = keepalive;
        self
    }
    
    /// 构建并验证配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
} 