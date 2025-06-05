use async_trait::async_trait;
use super::{SessionId, error::TransportError};
use super::command::{ConnectionInfo, TransportStats, ProtocolType, ConnectionState};
use super::packet::UnifiedPacket;

/// 适配器统计信息
#[derive(Debug, Clone)]
pub struct AdapterStats {
    /// 发送的数据包数量
    pub packets_sent: u64,
    /// 接收的数据包数量
    pub packets_received: u64,
    /// 发送的字节数
    pub bytes_sent: u64,
    /// 接收的字节数
    pub bytes_received: u64,
    /// 错误计数
    pub errors: u64,
    /// 最后活动时间
    pub last_activity: std::time::SystemTime,
}

impl Default for AdapterStats {
    fn default() -> Self {
        Self {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors: 0,
            last_activity: std::time::SystemTime::now(),
        }
    }
}

impl AdapterStats {
    pub fn new() -> Self {
        Default::default()
    }
    
    pub fn record_packet_sent(&mut self, size: usize) {
        self.packets_sent += 1;
        self.bytes_sent += size as u64;
        self.last_activity = std::time::SystemTime::now();
    }
    
    pub fn record_packet_received(&mut self, size: usize) {
        self.packets_received += 1;
        self.bytes_received += size as u64;
        self.last_activity = std::time::SystemTime::now();
    }
    
    pub fn record_error(&mut self) {
        self.errors += 1;
        self.last_activity = std::time::SystemTime::now();
    }
}

/// 协议适配器抽象接口
/// 
/// 此trait定义了所有协议适配器必须实现的基本功能
#[async_trait]
pub trait ProtocolAdapter: Send + 'static {
    type Config: ProtocolConfig;
    type Error: Into<TransportError> + Send + std::fmt::Debug + 'static;
    
    /// 发送数据包
    async fn send(&mut self, packet: UnifiedPacket) -> Result<(), Self::Error>;
    
    /// 接收数据包（非阻塞）
    async fn receive(&mut self) -> Result<Option<UnifiedPacket>, Self::Error>;
    
    /// 关闭连接
    async fn close(&mut self) -> Result<(), Self::Error>;
    
    /// 获取连接信息
    fn connection_info(&self) -> ConnectionInfo;
    
    /// 检查连接状态
    fn is_connected(&self) -> bool;
    
    /// 获取适配器统计信息
    fn stats(&self) -> AdapterStats;
    
    /// 获取会话ID
    fn session_id(&self) -> SessionId;
    
    /// 设置会话ID
    fn set_session_id(&mut self, session_id: SessionId);
    
    /// 检查是否有数据可读
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 默认实现：尝试非阻塞接收
        match self.receive().await {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(e),
        }
    }
    
    /// 刷新发送缓冲区
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 默认实现：空操作
        Ok(())
    }
}

/// 协议配置trait
pub trait ProtocolConfig: Send + Sync + Clone + std::fmt::Debug + 'static {
    /// 验证配置是否有效
    fn validate(&self) -> Result<(), ConfigError>;
    
    /// 获取默认配置
    fn default_config() -> Self;
    
    /// 合并配置
    fn merge(self, other: Self) -> Self;
}

/// 配置错误类型
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid value: {0}")]
    InvalidValue(String),
    
    #[error("Missing required field: {0}")]
    MissingField(String),
    
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// TCP适配器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TcpConfig {
    pub bind_address: std::net::SocketAddr,
    pub nodelay: bool,
    pub keepalive: Option<std::time::Duration>,
    pub read_buffer_size: usize,
    pub write_buffer_size: usize,
    pub connect_timeout: std::time::Duration,
    pub read_timeout: Option<std::time::Duration>,
    pub write_timeout: Option<std::time::Duration>,
}

impl Default for TcpConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

impl ProtocolConfig for TcpConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.read_buffer_size == 0 {
            return Err(ConfigError::InvalidValue("read_buffer_size must be > 0".to_string()));
        }
        
        if self.write_buffer_size == 0 {
            return Err(ConfigError::InvalidValue("write_buffer_size must be > 0".to_string()));
        }
        
        Ok(())
    }
    
    fn default_config() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            nodelay: true,
            keepalive: Some(std::time::Duration::from_secs(60)),
            read_buffer_size: 4096,
            write_buffer_size: 4096,
            connect_timeout: std::time::Duration::from_secs(10),
            read_timeout: Some(std::time::Duration::from_secs(30)),
            write_timeout: Some(std::time::Duration::from_secs(30)),
        }
    }
    
    fn merge(mut self, other: Self) -> Self {
        // 使用 other 的非默认值覆盖 self
        if other.bind_address.port() != 0 {
            self.bind_address = other.bind_address;
        }
        if !other.nodelay {
            self.nodelay = other.nodelay;
        }
        if other.keepalive.is_some() {
            self.keepalive = other.keepalive;
        }
        if other.read_buffer_size != 4096 {
            self.read_buffer_size = other.read_buffer_size;
        }
        if other.write_buffer_size != 4096 {
            self.write_buffer_size = other.write_buffer_size;
        }
        if other.connect_timeout != std::time::Duration::from_secs(10) {
            self.connect_timeout = other.connect_timeout;
        }
        if other.read_timeout.is_some() {
            self.read_timeout = other.read_timeout;
        }
        if other.write_timeout.is_some() {
            self.write_timeout = other.write_timeout;
        }
        self
    }
}

/// WebSocket适配器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WebSocketConfig {
    pub bind_address: std::net::SocketAddr,
    pub path: String,
    pub subprotocols: Vec<String>,
    pub max_frame_size: usize,
    pub max_message_size: usize,
    pub ping_interval: Option<std::time::Duration>,
    pub pong_timeout: std::time::Duration,
}

impl Default for WebSocketConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

impl ProtocolConfig for WebSocketConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_frame_size == 0 {
            return Err(ConfigError::InvalidValue("max_frame_size must be > 0".to_string()));
        }
        
        if self.max_message_size == 0 {
            return Err(ConfigError::InvalidValue("max_message_size must be > 0".to_string()));
        }
        
        if !self.path.starts_with('/') {
            return Err(ConfigError::InvalidValue("path must start with '/'".to_string()));
        }
        
        Ok(())
    }
    
    fn default_config() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            path: "/".to_string(),
            subprotocols: vec![],
            max_frame_size: 64 * 1024, // 64KB
            max_message_size: 1024 * 1024, // 1MB
            ping_interval: Some(std::time::Duration::from_secs(30)),
            pong_timeout: std::time::Duration::from_secs(10),
        }
    }
    
    fn merge(mut self, other: Self) -> Self {
        if other.bind_address.port() != 0 {
            self.bind_address = other.bind_address;
        }
        if other.path != "/" {
            self.path = other.path;
        }
        if !other.subprotocols.is_empty() {
            self.subprotocols = other.subprotocols;
        }
        if other.max_frame_size != 64 * 1024 {
            self.max_frame_size = other.max_frame_size;
        }
        if other.max_message_size != 1024 * 1024 {
            self.max_message_size = other.max_message_size;
        }
        if other.ping_interval.is_some() {
            self.ping_interval = other.ping_interval;
        }
        if other.pong_timeout != std::time::Duration::from_secs(10) {
            self.pong_timeout = other.pong_timeout;
        }
        self
    }
}

/// QUIC适配器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuicConfig {
    pub bind_address: std::net::SocketAddr,
    pub cert_path: Option<String>,
    pub key_path: Option<String>,
    pub max_concurrent_streams: u64,
    pub max_idle_timeout: std::time::Duration,
    pub keep_alive_interval: Option<std::time::Duration>,
    pub initial_rtt: std::time::Duration,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

impl ProtocolConfig for QuicConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_concurrent_streams == 0 {
            return Err(ConfigError::InvalidValue("max_concurrent_streams must be > 0".to_string()));
        }
        
        // 如果提供了证书路径，必须同时提供密钥路径
        match (&self.cert_path, &self.key_path) {
            (Some(_), None) => {
                return Err(ConfigError::MissingField("key_path is required when cert_path is provided".to_string()));
            }
            (None, Some(_)) => {
                return Err(ConfigError::MissingField("cert_path is required when key_path is provided".to_string()));
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn default_config() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            cert_path: None,
            key_path: None,
            max_concurrent_streams: 100,
            max_idle_timeout: std::time::Duration::from_secs(30),
            keep_alive_interval: Some(std::time::Duration::from_secs(15)),
            initial_rtt: std::time::Duration::from_millis(333),
        }
    }
    
    fn merge(mut self, other: Self) -> Self {
        if other.bind_address.port() != 0 {
            self.bind_address = other.bind_address;
        }
        if other.cert_path.is_some() {
            self.cert_path = other.cert_path;
        }
        if other.key_path.is_some() {
            self.key_path = other.key_path;
        }
        if other.max_concurrent_streams != 100 {
            self.max_concurrent_streams = other.max_concurrent_streams;
        }
        if other.max_idle_timeout != std::time::Duration::from_secs(30) {
            self.max_idle_timeout = other.max_idle_timeout;
        }
        if other.keep_alive_interval.is_some() {
            self.keep_alive_interval = other.keep_alive_interval;
        }
        if other.initial_rtt != std::time::Duration::from_millis(333) {
            self.initial_rtt = other.initial_rtt;
        }
        self
    }
} 