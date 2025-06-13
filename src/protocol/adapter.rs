use async_trait::async_trait;
use crate::{SessionId, error::TransportError};
use crate::command::ConnectionInfo;
use crate::packet::Packet;
use crate::protocol::{TcpServerConfig, TcpClientConfig, WebSocketServerConfig, WebSocketClientConfig, QuicServerConfig, QuicClientConfig};

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

/// 协议适配器trait
/// 
/// 定义了所有协议适配器必须实现的基本接口
/// 这是事件驱动架构的核心抽象
#[async_trait]
pub trait ProtocolAdapter: Send + 'static {
    type Config: ProtocolConfig;
    type Error: Into<TransportError> + Send + std::fmt::Debug + 'static;
    
    /// 发送数据包
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error>;
    
    /// 关闭连接
    async fn close(&mut self) -> Result<(), Self::Error>;
    
    /// 优雅关闭连接
    /// 
    /// 发送协议特定的关闭信号，等待对端确认
    async fn graceful_close(&mut self) -> Result<(), Self::Error> {
        // 默认实现：直接调用 close()
        self.close().await
    }
    
    /// 强制关闭连接
    /// 
    /// 立即关闭连接，不等待对端确认
    async fn force_close(&mut self) -> Result<(), Self::Error> {
        // 默认实现：直接调用 close()
        self.close().await
    }
    
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
    
    /// 刷新发送缓冲区
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 默认实现：在事件驱动模式下由内部事件循环处理
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

/// 🔧 服务端专用动态配置
pub trait DynServerConfig: DynProtocolConfig {
    /// 动态构建服务器（object-safe）
    fn build_server_dyn(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Box<dyn crate::Server>, crate::error::TransportError>> + Send + '_>>;
    
    /// 获取绑定地址
    fn get_bind_address(&self) -> std::net::SocketAddr;
    
    /// 克隆为 Box<dyn DynServerConfig>
    fn clone_server_dyn(&self) -> Box<dyn DynServerConfig>;
}

/// 🔧 客户端专用动态配置  
pub trait DynClientConfig: DynProtocolConfig {
    /// 动态构建连接（object-safe）
    fn build_connection_dyn(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Box<dyn crate::Connection>, crate::error::TransportError>> + Send + '_>>;
    
    /// 获取目标信息（可能是 SocketAddr 或 URL）
    fn get_target_info(&self) -> String;
    
    /// 克隆为 Box<dyn DynClientConfig>
    fn clone_client_dyn(&self) -> Box<dyn DynClientConfig>;
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


impl ServerConfig for TcpServerConfig {
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

impl ClientConfig for TcpClientConfig {
    type Connection = crate::connection::TcpConnection;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("TCP config validation failed: {:?}", e)))
    }

    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::tcp::TcpClientBuilder;
        
        let adapter = TcpClientBuilder::new()
            .target_address(self.target_address)
            .config(self.clone())
            .connect()
            .await
            .map_err(|e| TransportError::connection_error(format!("Failed to build TCP connection: {:?}", e), true))?;
            
        Ok(crate::connection::TcpConnection::new(adapter))
    }
    
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
}





impl ServerConfig for WebSocketServerConfig {
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

impl ClientConfig for WebSocketClientConfig {
    type Connection = crate::adapters::factories::WebSocketConnection;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("WebSocket config validation failed: {:?}", e)))
    }

    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::websocket::WebSocketClientBuilder;
        
        let adapter = WebSocketClientBuilder::new()
            .target_url(&self.target_url)
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





impl ServerConfig for QuicServerConfig {
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

impl ClientConfig for QuicClientConfig {
    type Connection = crate::adapters::factories::QuicConnection;
    
    fn validate(&self) -> Result<(), TransportError> {
        ProtocolConfig::validate(self).map_err(|e| TransportError::config_error("protocol", format!("QUIC config validation failed: {:?}", e)))
    }

    async fn build_connection(&self) -> Result<Self::Connection, TransportError> {
        use crate::adapters::quic::QuicClientBuilder;
        
        let adapter = QuicClientBuilder::new()
            .target_address(self.target_address)
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
    type Server: crate::Server;
    
    /// 验证配置的正确性
    fn validate(&self) -> Result<(), TransportError>;
    
    /// 构建服务器实例
    fn build_server(&self) -> impl std::future::Future<Output = Result<Self::Server, TransportError>> + Send;
    
    /// 获取协议名称
    fn protocol_name(&self) -> &'static str;
}

/// 客户端配置trait - 用于类型安全的客户端连接
pub trait ClientConfig: Send + Sync + 'static {
    type Connection: crate::Connection;
    
    /// 验证配置的正确性
    fn validate(&self) -> Result<(), TransportError>;
    
    /// 构建连接实例
    fn build_connection(&self) -> impl std::future::Future<Output = Result<Self::Connection, TransportError>> + Send;
    
    /// 获取协议名称
    fn protocol_name(&self) -> &'static str;
}

 

// ConnectableConfig 实现已移至 client_config.rs 中
