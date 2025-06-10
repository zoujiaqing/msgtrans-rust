//! 服务端配置模块 - 分离的服务端配置实现

use serde::{Serialize, Deserialize};
use std::time::Duration;
use crate::protocol::{ProtocolConfig, ConfigError};
use crate::protocol::adapter::DynProtocolConfig;

/// TCP服务端配置
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
    /// 服务器接受超时
    pub accept_timeout: Duration,
    /// 连接空闲超时
    pub idle_timeout: Option<Duration>,
    /// 是否允许端口复用
    pub reuse_port: bool,
    /// 是否允许地址复用
    pub reuse_addr: bool,
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
            accept_timeout: Duration::from_secs(30),
            idle_timeout: Some(Duration::from_secs(300)),
            reuse_port: false,
            reuse_addr: true,
        }
    }
}

impl ProtocolConfig for TcpServerConfig {
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
    
    fn default_config() -> Self {
        Self::default()
    }
    
    fn merge(mut self, other: Self) -> Self {
        // 简化的合并逻辑
        if other.bind_address.to_string() != "127.0.0.1:8080" {
            self.bind_address = other.bind_address;
        }
        if other.max_connections != 1000 {
            self.max_connections = other.max_connections;
        }
        self.nodelay = other.nodelay;
        if other.keepalive.is_some() {
            self.keepalive = other.keepalive;
        }
        self
    }
}

impl DynProtocolConfig for TcpServerConfig {
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

impl TcpServerConfig {
    /// 创建新的TCP服务端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置绑定地址
    pub fn with_bind_address<A: Into<std::net::SocketAddr>>(mut self, addr: A) -> Self {
        self.bind_address = addr.into();
        self
    }
    
    /// 设置最大连接数
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// 设置TCP_NODELAY选项
    pub fn with_nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }
    
    /// 设置keepalive时间
    pub fn with_keepalive(mut self, keepalive: Option<Duration>) -> Self {
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
    
    /// 设置接受超时
    pub fn with_accept_timeout(mut self, timeout: Duration) -> Self {
        self.accept_timeout = timeout;
        self
    }
    
    /// 设置连接空闲超时
    pub fn with_idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self
    }
    
    /// 设置端口复用
    pub fn with_reuse_port(mut self, reuse: bool) -> Self {
        self.reuse_port = reuse;
        self
    }
    
    /// 设置地址复用
    pub fn with_reuse_addr(mut self, reuse: bool) -> Self {
        self.reuse_addr = reuse;
        self
    }
    
    /// 构建配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
}

/// WebSocket服务端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketServerConfig {
    /// 绑定地址
    pub bind_address: std::net::SocketAddr,
    /// WebSocket路径
    pub path: String,
    /// 支持的子协议
    pub subprotocols: Vec<String>,
    /// 最大帧大小
    pub max_frame_size: usize,
    /// 最大消息大小
    pub max_message_size: usize,
    /// ping间隔
    pub ping_interval: Option<Duration>,
    /// pong超时
    pub pong_timeout: Duration,
    /// 最大连接数
    pub max_connections: usize,
    /// 连接空闲超时
    pub idle_timeout: Option<Duration>,
}

impl Default for WebSocketServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            path: "/".to_string(),
            subprotocols: vec![],
            max_frame_size: 16 * 1024 * 1024, // 16MB
            max_message_size: 64 * 1024 * 1024, // 64MB
            ping_interval: Some(Duration::from_secs(30)),
            pong_timeout: Duration::from_secs(10),
            max_connections: 1000,
            idle_timeout: Some(Duration::from_secs(300)),
        }
    }
}

impl ProtocolConfig for WebSocketServerConfig {
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
    
    fn default_config() -> Self {
        Self::default()
    }
    
    fn merge(mut self, other: Self) -> Self {
        if other.bind_address.to_string() != "127.0.0.1:8080" {
            self.bind_address = other.bind_address;
        }
        if other.path != "/" {
            self.path = other.path;
        }
        if !other.subprotocols.is_empty() {
            self.subprotocols = other.subprotocols;
        }
        self
    }
}

impl DynProtocolConfig for WebSocketServerConfig {
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

impl WebSocketServerConfig {
    /// 创建新的WebSocket服务端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置绑定地址
    pub fn with_bind_address<A: Into<std::net::SocketAddr>>(mut self, addr: A) -> Self {
        self.bind_address = addr.into();
        self
    }
    
    /// 设置WebSocket路径
    pub fn with_path<S: Into<String>>(mut self, path: S) -> Self {
        self.path = path.into();
        self
    }
    
    /// 设置支持的子协议
    pub fn with_subprotocols(mut self, protocols: Vec<String>) -> Self {
        self.subprotocols = protocols;
        self
    }
    
    /// 添加子协议
    pub fn add_subprotocol<S: Into<String>>(mut self, protocol: S) -> Self {
        self.subprotocols.push(protocol.into());
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
    pub fn with_ping_interval(mut self, interval: Option<Duration>) -> Self {
        self.ping_interval = interval;
        self
    }
    
    /// 设置pong超时
    pub fn with_pong_timeout(mut self, timeout: Duration) -> Self {
        self.pong_timeout = timeout;
        self
    }
    
    /// 设置最大连接数
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// 设置连接空闲超时
    pub fn with_idle_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.idle_timeout = timeout;
        self
    }
    
    /// 构建配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
}

/// QUIC服务端配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicServerConfig {
    /// 绑定地址
    pub bind_address: std::net::SocketAddr,
    /// TLS证书的PEM内容（可选，如果为None则自动生成自签名证书）
    pub cert_pem: Option<String>,
    /// TLS私钥的PEM内容（可选，如果为None则自动生成自签名证书）
    pub key_pem: Option<String>,
    /// 最大并发流数
    pub max_concurrent_streams: u64,
    /// 最大空闲超时
    pub max_idle_timeout: Duration,
    /// keepalive间隔
    pub keep_alive_interval: Option<Duration>,
    /// 初始RTT估值
    pub initial_rtt: Duration,
    /// 最大连接数
    pub max_connections: usize,
    /// 接收窗口大小
    pub receive_window: u32,
    /// 发送窗口大小
    pub send_window: u32,
}

impl Default for QuicServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            cert_pem: None,
            key_pem: None,
            max_concurrent_streams: 100,
            max_idle_timeout: Duration::from_secs(30),
            keep_alive_interval: Some(Duration::from_secs(15)),
            initial_rtt: Duration::from_millis(100),
            max_connections: 1000,
            receive_window: 1024 * 1024, // 1MB
            send_window: 1024 * 1024,    // 1MB
        }
    }
}

impl ProtocolConfig for QuicServerConfig {
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
    
    fn default_config() -> Self {
        Self::default()
    }
    
    fn merge(mut self, other: Self) -> Self {
        if other.bind_address.to_string() != "127.0.0.1:8080" {
            self.bind_address = other.bind_address;
        }
        if other.cert_pem.is_some() {
            self.cert_pem = other.cert_pem;
        }
        if other.key_pem.is_some() {
            self.key_pem = other.key_pem;
        }
        self
    }
}

impl DynProtocolConfig for QuicServerConfig {
    fn protocol_name(&self) -> &'static str {
        "quic"
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

impl QuicServerConfig {
    /// 创建新的QUIC服务端配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置绑定地址
    pub fn with_bind_address<A: Into<std::net::SocketAddr>>(mut self, addr: A) -> Self {
        self.bind_address = addr.into();
        self
    }
    
    /// 设置TLS证书PEM
    pub fn with_cert_pem<S: Into<String>>(mut self, cert_pem: S) -> Self {
        self.cert_pem = Some(cert_pem.into());
        self
    }
    
    /// 设置TLS私钥PEM
    pub fn with_key_pem<S: Into<String>>(mut self, key_pem: S) -> Self {
        self.key_pem = Some(key_pem.into());
        self
    }
    
    /// 设置最大并发流数
    pub fn with_max_concurrent_streams(mut self, count: u64) -> Self {
        self.max_concurrent_streams = count;
        self
    }
    
    /// 设置最大空闲超时
    pub fn with_max_idle_timeout(mut self, timeout: Duration) -> Self {
        self.max_idle_timeout = timeout;
        self
    }
    
    /// 设置keepalive间隔
    pub fn with_keep_alive_interval(mut self, interval: Option<Duration>) -> Self {
        self.keep_alive_interval = interval;
        self
    }
    
    /// 设置初始RTT估值
    pub fn with_initial_rtt(mut self, rtt: Duration) -> Self {
        self.initial_rtt = rtt;
        self
    }
    
    /// 设置最大连接数
    pub fn with_max_connections(mut self, max: usize) -> Self {
        self.max_connections = max;
        self
    }
    
    /// 设置接收窗口大小
    pub fn with_receive_window(mut self, window: u32) -> Self {
        self.receive_window = window;
        self
    }
    
    /// 设置发送窗口大小
    pub fn with_send_window(mut self, window: u32) -> Self {
        self.send_window = window;
        self
    }
    
    /// 构建配置
    pub fn build(self) -> Result<Self, ConfigError> {
        self.validate()?;
        Ok(self)
    }
    
    /// 创建测试用的不安全配置
    pub fn insecure() -> Self {
        Self::new()
            .with_cert_pem("") // 空证书表示使用自签名
            .with_key_pem("")  // 空私钥表示使用自签名
    }
}