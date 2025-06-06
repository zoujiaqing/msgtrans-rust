use std::path::Path;
use serde::{Serialize, Deserialize};
use crate::protocol::{ProtocolConfig, ConfigError, TcpConfig, WebSocketConfig, QuicConfig};

/// 配置构建器trait
pub trait ConfigBuilder<T: ProtocolConfig>: Default {
    /// 构建配置
    fn build(self) -> Result<T, ConfigError>;
    
    /// 从文件加载配置
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError>;
    
    /// 从环境变量加载配置
    fn from_env() -> Result<Self, ConfigError>;
    
    /// 从字符串解析配置
    fn from_str(s: &str) -> Result<Self, ConfigError>;
}

/// 顶层传输配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// 全局配置
    pub global: GlobalConfig,
    /// TCP配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tcp: Option<TcpConfig>,
    /// WebSocket配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub websocket: Option<WebSocketConfig>,
    /// QUIC配置
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quic: Option<QuicConfig>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            global: GlobalConfig::default(),
            tcp: None,
            websocket: None,
            quic: None,
        }
    }
}

impl TransportConfig {
    /// 创建新的传输配置
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 设置全局配置
    pub fn with_global(mut self, global: GlobalConfig) -> Self {
        self.global = global;
        self
    }
    
    /// 设置TCP配置
    pub fn with_tcp(mut self, tcp: TcpConfig) -> Self {
        self.tcp = Some(tcp);
        self
    }
    
    /// 设置WebSocket配置
    pub fn with_websocket(mut self, websocket: WebSocketConfig) -> Self {
        self.websocket = Some(websocket);
        self
    }
    
    /// 设置QUIC配置
    pub fn with_quic(mut self, quic: QuicConfig) -> Self {
        self.quic = Some(quic);
        self
    }
    
    /// 验证配置
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.global.validate()?;
        
        if let Some(ref tcp) = self.tcp {
            tcp.validate()?;
        }
        
        if let Some(ref websocket) = self.websocket {
            websocket.validate()?;
        }
        
        if let Some(ref quic) = self.quic {
            quic.validate()?;
        }
        
        Ok(())
    }
    
    /// 从文件加载配置
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }
    
    /// 从字符串解析配置
    pub fn from_str(s: &str) -> Result<Self, ConfigError> {
        // 尝试不同的格式
        if let Ok(config) = toml::from_str::<Self>(s) {
            config.validate()?;
            return Ok(config);
        }
        
        if let Ok(config) = serde_json::from_str::<Self>(s) {
            config.validate()?;
            return Ok(config);
        }
        
        if let Ok(config) = serde_yaml::from_str::<Self>(s) {
            config.validate()?;
            return Ok(config);
        }
        
        Err(ConfigError::ParseError("Unsupported config format".to_string()))
    }
    
    /// 合并配置
    pub fn merge(mut self, other: Self) -> Self {
        self.global = self.global.merge(other.global);
        
        if other.tcp.is_some() {
            self.tcp = match (self.tcp, other.tcp) {
                (Some(base), Some(other)) => Some(base.merge(other)),
                (None, Some(other)) => Some(other),
                (Some(base), None) => Some(base),
                (None, None) => None,
            };
        }
        
        if other.websocket.is_some() {
            self.websocket = match (self.websocket, other.websocket) {
                (Some(base), Some(other)) => Some(base.merge(other)),
                (None, Some(other)) => Some(other),
                (Some(base), None) => Some(base),
                (None, None) => None,
            };
        }
        
        if other.quic.is_some() {
            self.quic = match (self.quic, other.quic) {
                (Some(base), Some(other)) => Some(base.merge(other)),
                (None, Some(other)) => Some(other),
                (Some(base), None) => Some(base),
                (None, None) => None,
            };
        }
        
        self
    }
}

/// 全局配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// 最大连接数
    pub max_connections: usize,
    /// 缓冲区大小
    pub buffer_size: usize,
    /// 超时时间
    #[serde(with = "duration_serde")]
    pub timeout: std::time::Duration,
    /// 重试策略
    pub retry_policy: RetryPolicy,
    /// 日志级别
    pub log_level: LogLevel,
    /// 工作线程数
    pub worker_threads: Option<usize>,
    /// 是否启用压缩
    pub enable_compression: bool,
    /// 心跳间隔
    #[serde(with = "duration_serde")]
    pub heartbeat_interval: std::time::Duration,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            max_connections: 1000,
            buffer_size: 8192,
            timeout: std::time::Duration::from_secs(30),
            retry_policy: RetryPolicy::default(),
            log_level: LogLevel::Info,
            worker_threads: None,
            enable_compression: false,
            heartbeat_interval: std::time::Duration::from_secs(60),
        }
    }
}

impl GlobalConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == 0 {
            return Err(ConfigError::InvalidValue("max_connections must be > 0".to_string()));
        }
        
        if self.buffer_size == 0 {
            return Err(ConfigError::InvalidValue("buffer_size must be > 0".to_string()));
        }
        
        Ok(())
    }
    
    pub fn merge(mut self, other: Self) -> Self {
        if other.max_connections != 1000 {
            self.max_connections = other.max_connections;
        }
        if other.buffer_size != 8192 {
            self.buffer_size = other.buffer_size;
        }
        if other.timeout != std::time::Duration::from_secs(30) {
            self.timeout = other.timeout;
        }
        self.retry_policy = self.retry_policy.merge(other.retry_policy);
        if other.log_level != LogLevel::Info {
            self.log_level = other.log_level;
        }
        if other.worker_threads.is_some() {
            self.worker_threads = other.worker_threads;
        }
        if other.enable_compression {
            self.enable_compression = other.enable_compression;
        }
        if other.heartbeat_interval != std::time::Duration::from_secs(60) {
            self.heartbeat_interval = other.heartbeat_interval;
        }
        self
    }
}

/// 重试策略配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// 最大重试次数
    pub max_attempts: u32,
    /// 基础延迟时间
    #[serde(with = "duration_serde")]
    pub base_delay: std::time::Duration,
    /// 最大延迟时间
    #[serde(with = "duration_serde")]
    pub max_delay: std::time::Duration,
    /// 退避乘数
    pub backoff_multiplier: f64,
    /// 是否启用抖动
    pub enable_jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: std::time::Duration::from_millis(100),
            max_delay: std::time::Duration::from_secs(60),
            backoff_multiplier: 2.0,
            enable_jitter: true,
        }
    }
}

impl RetryPolicy {
    pub fn merge(mut self, other: Self) -> Self {
        if other.max_attempts != 3 {
            self.max_attempts = other.max_attempts;
        }
        if other.base_delay != std::time::Duration::from_millis(100) {
            self.base_delay = other.base_delay;
        }
        if other.max_delay != std::time::Duration::from_secs(60) {
            self.max_delay = other.max_delay;
        }
        if (other.backoff_multiplier - 2.0).abs() > f64::EPSILON {
            self.backoff_multiplier = other.backoff_multiplier;
        }
        if !other.enable_jitter {
            self.enable_jitter = other.enable_jitter;
        }
        self
    }
}

/// 日志级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogLevel {
    #[serde(rename = "trace")]
    Trace,
    #[serde(rename = "debug")]
    Debug,
    #[serde(rename = "info")]
    Info,
    #[serde(rename = "warn")]
    Warn,
    #[serde(rename = "error")]
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Trace => write!(f, "trace"),
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

/// TCP配置构建器
#[derive(Default)]
pub struct TcpConfigBuilder {
    inner: TcpConfig,
}

impl TcpConfigBuilder {
    pub fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.inner.bind_address = addr;
        self
    }
    
    pub fn nodelay(mut self, nodelay: bool) -> Self {
        self.inner.nodelay = nodelay;
        self
    }
    
    pub fn keepalive(mut self, keepalive: Option<std::time::Duration>) -> Self {
        self.inner.keepalive = keepalive;
        self
    }
    
    pub fn read_buffer_size(mut self, size: usize) -> Self {
        self.inner.read_buffer_size = size;
        self
    }
    
    pub fn write_buffer_size(mut self, size: usize) -> Self {
        self.inner.write_buffer_size = size;
        self
    }
    
    pub fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.inner.connect_timeout = timeout;
        self
    }
}

impl ConfigBuilder<TcpConfig> for TcpConfigBuilder {
    fn build(self) -> Result<TcpConfig, ConfigError> {
        self.inner.validate()?;
        Ok(self.inner)
    }
    
    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        Self::from_str(&content)
    }
    
    fn from_env() -> Result<Self, ConfigError> {
        let mut builder = Self::default();
        
        if let Ok(addr) = std::env::var("TCP_BIND_ADDRESS") {
            let addr = addr.parse().map_err(|e| ConfigError::ParseError(format!("Invalid bind address: {}", e)))?;
            builder = builder.bind_address(addr);
        }
        
        if let Ok(nodelay) = std::env::var("TCP_NODELAY") {
            let nodelay = nodelay.parse().map_err(|e| ConfigError::ParseError(format!("Invalid nodelay: {}", e)))?;
            builder = builder.nodelay(nodelay);
        }
        
        Ok(builder)
    }
    
    fn from_str(s: &str) -> Result<Self, ConfigError> {
        let config: TcpConfig = toml::from_str(s)
            .or_else(|_| serde_json::from_str(s))
            .or_else(|_| serde_yaml::from_str(s))
            .map_err(|e| ConfigError::ParseError(format!("Parse error: {}", e)))?;
        
        Ok(Self { inner: config })
    }
}

/// Duration序列化模块
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;
    
    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs().serialize(serializer)
    }
    
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(secs))
    }
} 