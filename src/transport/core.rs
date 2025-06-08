/// Transport核心API - Phase 1实现
/// 
/// 这是msgtrans的核心传输层接口，提供协议透明的统一API

use std::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use bytes::Bytes;

use crate::error::TransportError;
use crate::SessionId;

/// 传输层核心接口
pub struct Transport {
    inner: Arc<TransportInner>,
}

/// Transport内部实现
struct TransportInner {
    /// 协议适配器映射
    adapters: RwLock<HashMap<String, Arc<dyn ProtocolAdapter>>>,
    /// 活跃会话映射
    sessions: RwLock<HashMap<SessionId, Arc<Session>>>,
    /// 连接池配置
    pool_config: PoolConfig,
    /// 传输层配置
    config: TransportConfig,
}

/// 会话抽象 - 协议透明
#[derive(Debug)]
pub struct Session {
    pub id: SessionId,
    pub protocol: Protocol,
    pub remote_addr: Option<std::net::SocketAddr>,
    pub local_addr: Option<std::net::SocketAddr>,
    pub connected: std::sync::atomic::AtomicBool,
}

/// 协议枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protocol {
    Tcp,
    WebSocket,
    Quic,
}

impl std::fmt::Display for Protocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "tcp"),
            Protocol::WebSocket => write!(f, "websocket"), 
            Protocol::Quic => write!(f, "quic"),
        }
    }
}

/// 连接配置统一接口
#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    Tcp(TcpConfig),
    WebSocket(WebSocketConfig),
    Quic(QuicConfig),
}

/// TCP连接配置
#[derive(Debug, Clone)]
pub struct TcpConfig {
    pub addr: std::net::SocketAddr,
    pub nodelay: bool,
    pub keepalive: Option<Duration>,
}

/// WebSocket连接配置
#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
    pub subprotocols: Vec<String>,
}

/// QUIC连接配置
#[derive(Debug, Clone)]
pub struct QuicConfig {
    pub addr: std::net::SocketAddr,
    pub server_name: Option<String>,
    pub cert_verification: bool,
}

/// 连接池配置
#[derive(Debug, Clone)]
pub struct PoolConfig {
    pub max_size: usize,
    pub current_size: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

/// 连接池状态
#[derive(Debug, Clone)]
pub struct PoolStatus {
    pub active_connections: usize,
    pub idle_connections: usize,
    pub pool_utilization: f64,
    pub expansion_count: u64,
    pub shrink_count: u64,
}

/// 传输层配置
#[derive(Debug, Clone)]
pub struct TransportConfig {
    pub connect_timeout: Duration,
    pub send_timeout: Duration,
    pub receive_timeout: Duration,
    pub max_message_size: usize,
}

/// Transport实现
impl Transport {
    /// 创建构建器
    pub fn builder() -> TransportBuilder {
        TransportBuilder::new()
    }

    /// 建立连接 - 核心API
    pub async fn connect(&self, config: ConnectionConfig) -> Result<Session, TransportError> {
        let protocol = match &config {
            ConnectionConfig::Tcp(_) => Protocol::Tcp,
            ConnectionConfig::WebSocket(_) => Protocol::WebSocket,
            ConnectionConfig::Quic(_) => Protocol::Quic,
        };

        // 获取对应的协议适配器
        let adapters = self.inner.adapters.read().await;
        let adapter = adapters
            .get(&protocol.to_string())
            .ok_or_else(|| TransportError::protocol_error(&protocol.to_string(), "Protocol adapter not found"))?;

        // 通过适配器建立连接
        let session_info = adapter.connect(config).await?;
        
        // 生成会话ID
        let session_id = SessionId::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64
        );
        
        // 创建会话对象
        let session = Arc::new(Session {
            id: session_id,
            protocol: protocol.clone(),
            remote_addr: session_info.remote_addr,
            local_addr: session_info.local_addr,
            connected: std::sync::atomic::AtomicBool::new(true),
        });

        // 注册会话
        {
            let mut sessions = self.inner.sessions.write().await;
            sessions.insert(session_id, session.clone());
        }

        Ok(Session {
            id: session_id,
            protocol,
            remote_addr: session_info.remote_addr,
            local_addr: session_info.local_addr,
            connected: std::sync::atomic::AtomicBool::new(true),
        })
    }

    /// 发送消息 - 核心API
    pub async fn send(&self, session: &Session, data: Bytes) -> Result<(), TransportError> {
        // 检查会话状态
        if !session.is_connected() {
            return Err(TransportError::connection_error("Session not connected", false));
        }

        // 获取协议适配器
        let adapters = self.inner.adapters.read().await;
        let adapter = adapters
            .get(&session.protocol.to_string())
            .ok_or_else(|| TransportError::protocol_error(&session.protocol.to_string(), "Protocol adapter not found"))?;

        // 通过适配器发送数据
        adapter.send(session.id, data).await
    }

    /// 接收消息 - 核心API
    pub async fn receive(&self, session: &Session) -> Result<Bytes, TransportError> {
        // 检查会话状态
        if !session.is_connected() {
            return Err(TransportError::connection_error("Session not connected", false));
        }

        // 获取协议适配器
        let adapters = self.inner.adapters.read().await;
        let adapter = adapters
            .get(&session.protocol.to_string())
            .ok_or_else(|| TransportError::protocol_error(&session.protocol.to_string(), "Protocol adapter not found"))?;

        // 通过适配器接收数据
        adapter.receive(session.id).await
    }

    /// 获取连接池状态
    pub fn pool_status(&self) -> PoolStatus {
        let current_size = self.inner.pool_config.current_size.load(std::sync::atomic::Ordering::Relaxed);
        let max_size = self.inner.pool_config.max_size;
        
        PoolStatus {
            active_connections: current_size,
            idle_connections: 0, // TODO: 实际计算空闲连接数
            pool_utilization: current_size as f64 / max_size as f64,
            expansion_count: 0, // TODO: 添加实际统计
            shrink_count: 0,    // TODO: 添加实际统计
        }
    }

    /// 获取活跃会话数量
    pub async fn session_count(&self) -> usize {
        self.inner.sessions.read().await.len()
    }
}

/// Session实现
impl Session {
    pub fn id(&self) -> SessionId {
        self.id
    }

    pub fn protocol(&self) -> Protocol {
        self.protocol.clone()
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn local_addr(&self) -> Option<std::net::SocketAddr> {
        self.local_addr
    }

    pub fn remote_addr(&self) -> Option<std::net::SocketAddr> {
        self.remote_addr
    }
}

/// 会话信息
#[derive(Debug)]
pub struct SessionInfo {
    pub remote_addr: Option<std::net::SocketAddr>,
    pub local_addr: Option<std::net::SocketAddr>,
}

/// 协议适配器trait - 内部接口
#[async_trait::async_trait]
pub trait ProtocolAdapter: Send + Sync {
    async fn connect(&self, config: ConnectionConfig) -> Result<SessionInfo, TransportError>;
    async fn send(&self, session_id: SessionId, data: Bytes) -> Result<(), TransportError>;
    async fn receive(&self, session_id: SessionId) -> Result<Bytes, TransportError>;
}

/// Transport构建器
pub struct TransportBuilder {
    pool_config: PoolConfig,
    transport_config: TransportConfig,
    adapters: Vec<(String, Arc<dyn ProtocolAdapter>)>,
}

impl TransportBuilder {
    pub fn new() -> Self {
        Self {
            pool_config: PoolConfig {
                max_size: 1000,
                current_size: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            },
            transport_config: TransportConfig {
                connect_timeout: Duration::from_secs(30),
                send_timeout: Duration::from_secs(30),
                receive_timeout: Duration::from_secs(30),
                max_message_size: 1024 * 1024, // 1MB
            },
            adapters: Vec::new(),
        }
    }

    /// 设置连接池配置
    pub fn connection_pool(mut self, max_size: usize) -> Self {
        self.pool_config.max_size = max_size;
        self
    }

    /// 设置连接超时
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.transport_config.connect_timeout = timeout;
        self
    }

    /// 设置发送超时
    pub fn send_timeout(mut self, timeout: Duration) -> Self {
        self.transport_config.send_timeout = timeout;
        self
    }

    /// 添加协议适配器
    pub fn with_adapter(mut self, protocol: impl Into<String>, adapter: Arc<dyn ProtocolAdapter>) -> Self {
        self.adapters.push((protocol.into(), adapter));
        self
    }

    /// 构建Transport实例
    pub async fn build(self) -> Result<Transport, TransportError> {
        let mut adapter_map = HashMap::new();
        
        // 注册适配器
        for (protocol, adapter) in self.adapters {
            adapter_map.insert(protocol, adapter);
        }

        let inner = Arc::new(TransportInner {
            adapters: RwLock::new(adapter_map),
            sessions: RwLock::new(HashMap::new()),
            pool_config: self.pool_config,
            config: self.transport_config,
        });

        Ok(Transport { inner })
    }
}

impl Default for TransportBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// ConnectionConfig的Into trait实现
impl From<TcpConfig> for ConnectionConfig {
    fn from(config: TcpConfig) -> Self {
        ConnectionConfig::Tcp(config)
    }
}

impl From<WebSocketConfig> for ConnectionConfig {
    fn from(config: WebSocketConfig) -> Self {
        ConnectionConfig::WebSocket(config)
    }
}

impl From<QuicConfig> for ConnectionConfig {
    fn from(config: QuicConfig) -> Self {
        ConnectionConfig::Quic(config)
    }
}

/// TcpConfig便利构造函数
impl TcpConfig {
    pub fn new(addr: impl Into<std::net::SocketAddr>) -> Self {
        Self {
            addr: addr.into(),
            nodelay: true,
            keepalive: Some(Duration::from_secs(30)),
        }
    }

    pub fn with_nodelay(mut self, nodelay: bool) -> Self {
        self.nodelay = nodelay;
        self
    }

    pub fn with_keepalive(mut self, keepalive: Option<Duration>) -> Self {
        self.keepalive = keepalive;
        self
    }
}

/// WebSocketConfig便利构造函数
impl WebSocketConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: None,
            subprotocols: Vec::new(),
        }
    }

    pub fn with_headers(mut self, headers: std::collections::HashMap<String, String>) -> Self {
        self.headers = Some(headers);
        self
    }

    pub fn with_subprotocols(mut self, subprotocols: Vec<String>) -> Self {
        self.subprotocols = subprotocols;
        self
    }
}

/// QuicConfig便利构造函数
impl QuicConfig {
    pub fn new(addr: impl Into<std::net::SocketAddr>) -> Self {
        Self {
            addr: addr.into(),
            server_name: None,
            cert_verification: true,
        }
    }

    pub fn with_server_name(mut self, server_name: impl Into<String>) -> Self {
        self.server_name = Some(server_name.into());
        self
    }

    pub fn with_cert_verification(mut self, verify: bool) -> Self {
        self.cert_verification = verify;
        self
    }
} 