use async_trait::async_trait;
use crate::{SessionId, error::TransportError};
use crate::command::ConnectionInfo;
use crate::packet::Packet;

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
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error>;
    
    /// 接收数据包（非阻塞）
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error>;
    
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

/// Object-safe 的协议配置 trait，用于统一 Builder 接口
pub trait DynProtocolConfig: Send + Sync + 'static {
    /// 获取协议名称
    fn protocol_name(&self) -> &'static str;
    
    /// 验证配置
    fn validate_dyn(&self) -> Result<(), ConfigError>;
    
    /// 转换为 Any 以支持向下转型
    fn as_any(&self) -> &dyn std::any::Any;
    
    /// 克隆为 Box<dyn DynProtocolConfig>
    fn clone_dyn(&self) -> Box<dyn DynProtocolConfig>;
}

/// 协议配置错误
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Invalid address '{address}': {reason}")]
    InvalidAddress { 
        address: String, 
        reason: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
    
    #[error("Invalid port {port}: {reason}\nSuggestion: Use a port between 1 and 65535")]
    InvalidPort { 
        port: u32,
        reason: String,
    },
    
    #[error("Missing required field '{field}'\nSuggestion: {suggestion}")]
    MissingRequiredField { 
        field: String,
        suggestion: String,
    },
    
    #[error("Invalid value for '{field}': {value}\nReason: {reason}\nSuggestion: {suggestion}")]
    InvalidValue { 
        field: String,
        value: String,
        reason: String,
        suggestion: String,
    },
    
    #[error("File not found: '{path}'\nSuggestion: {suggestion}")]
    FileNotFound { 
        path: String,
        suggestion: String,
    },
    
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
            return Err(ConfigError::InvalidValue {
                field: "read_buffer_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value".to_string(),
            });
        }
        
        if self.write_buffer_size == 0 {
            return Err(ConfigError::InvalidValue {
                field: "write_buffer_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value".to_string(),
            });
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

impl TcpConfig {
    /// 创建新的TCP配置，立即验证
    pub fn new(bind_addr: &str) -> Result<Self, ConfigError> {
        let bind_address = bind_addr.parse()
            .map_err(|e| ConfigError::InvalidAddress {
                address: bind_addr.to_string(),
                reason: format!("Invalid bind address: {}", e),
                source: Some(Box::new(e)),
            })?;
        
        let config = Self {
            bind_address,
            ..Self::default()
        };
        
        // 立即验证 - 使用显式的trait调用
        ProtocolConfig::validate(&config)?;
        Ok(config)
    }
    
    /// 设置TCP_NODELAY选项
    pub fn with_nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }
    
    /// 设置keepalive时间
    pub fn with_keepalive(mut self, keepalive: Option<std::time::Duration>) -> Self {
        self.keepalive = keepalive;
        self
    }
    
    /// 设置读缓冲区大小
    pub fn with_read_buffer_size(mut self, size: usize) -> Self {
        self.read_buffer_size = size;
        self
    }
    
    /// 设置写缓冲区大小
    pub fn with_write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }
    
    /// 设置连接超时时间
    pub fn with_connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }
    
    /// 设置读超时时间
    pub fn with_read_timeout(mut self, timeout: Option<std::time::Duration>) -> Self {
        self.read_timeout = timeout;
        self
    }
    
    /// 设置写超时时间
    pub fn with_write_timeout(mut self, timeout: Option<std::time::Duration>) -> Self {
        self.write_timeout = timeout;
        self
    }
    
    /// 创建高性能配置预设
    pub fn high_performance(bind_addr: &str) -> Result<Self, ConfigError> {
        Ok(Self::new(bind_addr)?
            .with_nodelay(true)
            .with_read_buffer_size(65536)
            .with_write_buffer_size(65536)
            .with_keepalive(Some(std::time::Duration::from_secs(30))))
    }
    
    /// 创建低延迟配置预设
    pub fn low_latency(bind_addr: &str) -> Result<Self, ConfigError> {
        Ok(Self::new(bind_addr)?
            .with_nodelay(true)
            .with_read_buffer_size(4096)
            .with_write_buffer_size(4096))
    }
    
    /// 创建高吞吐量配置预设
    pub fn high_throughput(bind_addr: &str) -> Result<Self, ConfigError> {
        Ok(Self::new(bind_addr)?
            .with_read_buffer_size(131072)  // 128KB
            .with_write_buffer_size(131072)
            .with_keepalive(Some(std::time::Duration::from_secs(60))))
    }
}

impl ServerConfig for TcpConfig {
    type Server = crate::adapters::factories::TcpServerWrapper;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("TCP config validation failed: {:?}", e)))
    }
    
    async fn build_server(&self) -> Result<Self::Server, TransportError> {
        use crate::adapters::tcp::TcpServerBuilder;
        
        let server = TcpServerBuilder::new()
            .bind_address(self.bind_address)
            .config(self.clone())
            .build()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build TCP server: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::TcpServerWrapper::new(server))
    }
    
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
}

impl ClientConfig for TcpConfig {
    type Connection = crate::adapters::factories::TcpConnection;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("TCP config validation failed: {:?}", e)))
    }
    
    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::tcp::TcpClientBuilder;
        
        let adapter = TcpClientBuilder::new()
            .target_address(self.bind_address) // 对于客户端，bind_address作为目标地址
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build TCP connection: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::TcpConnection::new(adapter))
    }
    
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
}

impl DynProtocolConfig for TcpConfig {
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
    
    fn validate_dyn(&self) -> Result<(), ConfigError> {
        ProtocolConfig::validate(self)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    fn clone_dyn(&self) -> Box<dyn DynProtocolConfig> {
        Box::new(self.clone())
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
            return Err(ConfigError::InvalidValue {
                field: "max_frame_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value".to_string(),
            });
        }
        
        if self.max_message_size == 0 {
            return Err(ConfigError::InvalidValue {
                field: "max_message_size".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value".to_string(),
            });
        }
        
        if !self.path.starts_with('/') {
            return Err(ConfigError::InvalidValue {
                field: "path".to_string(),
                value: format!("\"{}\"", self.path),
                reason: "must start with '/'".to_string(),
                suggestion: "use a path like '/ws' or '/api/websocket'".to_string(),
            });
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

impl WebSocketConfig {
    /// 创建新的WebSocket配置，立即验证
    pub fn new(bind_addr: &str) -> Result<Self, ConfigError> {
        let bind_address = bind_addr.parse()
            .map_err(|e| ConfigError::InvalidAddress {
                address: bind_addr.to_string(),
                reason: format!("Invalid bind address: {}", e),
                source: Some(Box::new(e)),
            })?;
        
        let config = Self {
            bind_address,
            ..Self::default()
        };
        
        // 立即验证 - 使用显式的trait调用
        ProtocolConfig::validate(&config)?;
        Ok(config)
    }
    
    /// 设置WebSocket路径（为了链式调用一致性，改为返回Self）
    pub fn with_path<S: Into<String>>(mut self, path: S) -> Self {
        let path_str = path.into();
        if !path_str.starts_with('/') {
            // 对于链式调用，我们使用默认路径，并输出警告而不是报错
            eprintln!("Warning: WebSocket path '{}' should start with '/', using default '/ws'", path_str);
            self.path = "/ws".to_string();
        } else {
            self.path = path_str;
        }
        self
    }
    
    /// 设置支持的子协议
    pub fn with_subprotocols(mut self, subprotocols: Vec<String>) -> Self {
        self.subprotocols = subprotocols;
        self
    }
    
    /// 设置最大帧大小
    pub fn with_max_frame_size(mut self, size: usize) -> Self {
        self.max_frame_size = size;
        self
    }
    
    /// 设置最大消息大小
    pub fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }
    
    /// 设置ping间隔
    pub fn with_ping_interval(mut self, interval: Option<std::time::Duration>) -> Self {
        self.ping_interval = interval;
        self
    }
    
    /// 设置pong超时时间
    pub fn with_pong_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.pong_timeout = timeout;
        self
    }
}

impl ServerConfig for WebSocketConfig {
    type Server = crate::adapters::factories::WebSocketServerWrapper;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("WebSocket config validation failed: {:?}", e)))
    }
    
    async fn build_server(&self) -> Result<Self::Server, TransportError> {
        use crate::adapters::websocket::WebSocketServerBuilder;
        
        let server = WebSocketServerBuilder::new()
            .bind_address(self.bind_address)
            .config(self.clone())
            .build()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build WebSocket server: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::WebSocketServerWrapper::new(server))
    }
    
    fn protocol_name(&self) -> &'static str {
        "websocket"
    }
}

impl ClientConfig for WebSocketConfig {
    type Connection = crate::adapters::factories::WebSocketConnection<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("WebSocket config validation failed: {:?}", e)))
    }
    
    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::websocket::WebSocketClientBuilder;
        
        // 构建WebSocket URL
        let url = format!("ws://{}{}", self.bind_address, self.path);
        
        let adapter = WebSocketClientBuilder::new()
            .target_url(&url)
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build WebSocket connection: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::WebSocketConnection::new(adapter))
    }
    
    fn protocol_name(&self) -> &'static str {
        "websocket"
    }
}

impl DynProtocolConfig for WebSocketConfig {
    fn protocol_name(&self) -> &'static str {
        "websocket"
    }
    
    fn validate_dyn(&self) -> Result<(), ConfigError> {
        ProtocolConfig::validate(self)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    
    fn clone_dyn(&self) -> Box<dyn DynProtocolConfig> {
        Box::new(self.clone())
    }
}

/// QUIC适配器配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QuicConfig {
    pub bind_address: std::net::SocketAddr,
    /// TLS证书的PEM内容（可选，如果为None则自动生成自签名证书）
    pub cert_pem: Option<String>,
    /// TLS私钥的PEM内容（可选，如果为None则自动生成自签名证书）
    pub key_pem: Option<String>,
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
            return Err(ConfigError::InvalidValue {
                field: "max_concurrent_streams".to_string(),
                value: "0".to_string(),
                reason: "must be > 0".to_string(),
                suggestion: "set a positive value".to_string(),
            });
        }
        
        // 如果提供了证书PEM，必须同时提供密钥PEM
        match (&self.cert_pem, &self.key_pem) {
            (Some(_), None) => {
                return Err(ConfigError::MissingRequiredField {
                    field: "key_pem".to_string(),
                    suggestion: "provide a key_pem when cert_pem is provided".to_string(),
                });
            }
            (None, Some(_)) => {
                return Err(ConfigError::MissingRequiredField {
                    field: "cert_pem".to_string(),
                    suggestion: "provide a cert_pem when key_pem is provided".to_string(),
                });
            }
            _ => {}
        }
        
        Ok(())
    }
    
    fn default_config() -> Self {
        Self {
            bind_address: "0.0.0.0:0".parse().unwrap(),
            cert_pem: None,
            key_pem: None,
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
        if other.cert_pem.is_some() {
            self.cert_pem = other.cert_pem;
        }
        if other.key_pem.is_some() {
            self.key_pem = other.key_pem;
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

impl QuicConfig {
    /// 创建新的QUIC配置，立即验证
    pub fn new(bind_addr: &str) -> Result<Self, ConfigError> {
        let bind_address = bind_addr.parse()
            .map_err(|e| ConfigError::InvalidAddress {
                address: bind_addr.to_string(),
                reason: format!("Invalid bind address: {}", e),
                source: Some(Box::new(e)),
            })?;
        
        let config = Self {
            bind_address,
            ..Self::default()
        };
        
        // 立即验证 - 使用显式的trait调用
        ProtocolConfig::validate(&config)?;
        Ok(config)
    }
    
    /// 设置证书PEM内容
    pub fn with_cert_pem<S: Into<String>>(mut self, cert_pem: S) -> Self {
        self.cert_pem = Some(cert_pem.into());
        self
    }
    
    /// 设置私钥PEM内容
    pub fn with_key_pem<S: Into<String>>(mut self, key_pem: S) -> Self {
        self.key_pem = Some(key_pem.into());
        self
    }
    
    /// 同时设置证书和私钥PEM内容（安全模式）
    pub fn with_tls_cert<S1: Into<String>, S2: Into<String>>(mut self, cert_pem: S1, key_pem: S2) -> Self {
        self.cert_pem = Some(cert_pem.into());
        self.key_pem = Some(key_pem.into());
        self
    }
    
    /// 设置最大空闲时间
    pub fn with_max_idle_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.max_idle_timeout = timeout;
        self
    }
    
    /// 设置keepalive间隔
    pub fn with_keep_alive_interval(mut self, interval: Option<std::time::Duration>) -> Self {
        self.keep_alive_interval = interval;
        self
    }
    
    /// 设置最大并发流数
    pub fn with_max_concurrent_streams(mut self, count: u64) -> Self {
        self.max_concurrent_streams = count;
        self
    }
    
    /// 设置初始RTT估值
    pub fn with_initial_rtt(mut self, rtt: std::time::Duration) -> Self {
        self.initial_rtt = rtt;
        self
    }
}

impl ServerConfig for QuicConfig {
    type Server = crate::adapters::factories::QuicServerWrapper;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("QUIC config validation failed: {:?}", e)))
    }
    
    async fn build_server(&self) -> Result<Self::Server, TransportError> {
        use crate::adapters::quic::QuicServerBuilder;
        
        let server = QuicServerBuilder::new()
            .bind_address(self.bind_address)
            .config(self.clone())
            .build()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build QUIC server: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::QuicServerWrapper::new(server))
    }
    
    fn protocol_name(&self) -> &'static str {
        "quic"
    }
}

impl ClientConfig for QuicConfig {
    type Connection = crate::adapters::factories::QuicConnection;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("QUIC config validation failed: {:?}", e)))
    }
    
    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::quic::QuicClientBuilder;
        
        let adapter = QuicClientBuilder::new()
            .target_address(self.bind_address) // 对于客户端，bind_address作为目标地址
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build QUIC connection: {:?}", e), true))?;
            
        Ok(crate::adapters::factories::QuicConnection::new(adapter))
    }
    
    fn protocol_name(&self) -> &'static str {
        "quic"
    }
}

/// 服务器配置trait - 用于类型安全的服务器启动
pub trait ServerConfig: Send + Sync + 'static {
    type Server: crate::protocol::Server;
    
    /// 验证配置的正确性
    fn validate(&self) -> Result<(), TransportError>;
    
    /// 构建服务器实例
    fn build_server(&self) -> impl std::future::Future<Output = Result<Self::Server, TransportError>> + Send;
    
    /// 获取协议名称
    fn protocol_name(&self) -> &'static str;
}

/// 客户端配置trait - 用于类型安全的客户端连接
pub trait ClientConfig: Send + Sync + 'static {
    type Connection: crate::protocol::Connection;
    
    /// 验证配置的正确性
    fn validate(&self) -> Result<(), TransportError>;
    
    /// 构建连接实例
    fn build_connection(&self) -> impl std::future::Future<Output = Result<Self::Connection, TransportError>> + Send;
    
    /// 获取协议名称
    fn protocol_name(&self) -> &'static str;
}

  impl DynProtocolConfig for QuicConfig { fn protocol_name(&self) -> &'static str { "quic" } fn validate_dyn(&self) -> Result<(), ConfigError> { ProtocolConfig::validate(self) } fn as_any(&self) -> &dyn std::any::Any { self } fn clone_dyn(&self) -> Box<dyn DynProtocolConfig> { Box::new(self.clone()) } } 

// ========== ConnectableConfig 实现 ==========
// 让每个协议配置自己知道如何建立连接，避免硬编码协议判断

#[async_trait::async_trait]
impl crate::transport::api::ConnectableConfig for TcpConfig {
    async fn connect(&self, transport: &crate::transport::api::Transport) -> Result<crate::SessionId, crate::TransportError> {
        use crate::adapters::tcp::TcpClientBuilder;
        
        let adapter = TcpClientBuilder::new()
            .target_address(self.bind_address)
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| crate::TransportError::connection_error(format!("TCP connection failed: {:?}", e), true))?;
        
        transport.add_connection(adapter).await
    }
}

#[cfg(feature = "websocket")]
#[async_trait::async_trait]
impl crate::transport::api::ConnectableConfig for WebSocketConfig {
    async fn connect(&self, transport: &crate::transport::api::Transport) -> Result<crate::SessionId, crate::TransportError> {
        use crate::adapters::websocket::WebSocketClientBuilder;
        
        let url = format!("ws://{}", self.bind_address);
        let adapter = WebSocketClientBuilder::new()
            .target_url(&url)
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| crate::TransportError::connection_error(format!("WebSocket connection failed: {:?}", e), true))?;
        
        transport.add_connection(adapter).await
    }
}

#[cfg(feature = "quic")]
#[async_trait::async_trait]
impl crate::transport::api::ConnectableConfig for QuicConfig {
    async fn connect(&self, transport: &crate::transport::api::Transport) -> Result<crate::SessionId, crate::TransportError> {
        use crate::adapters::quic::QuicClientBuilder;
        
        let adapter = QuicClientBuilder::new()
            .target_address(self.bind_address)
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| crate::TransportError::connection_error(format!("QUIC connection failed: {:?}", e), true))?;
        
        transport.add_connection(adapter).await
    }
}
