/// 协议工厂实现
/// 
/// 为现有的协议适配器提供工厂接口实现

use std::any::Any;
use std::collections::HashMap;
use async_trait::async_trait;
use crate::protocol::{Connection, Server, ProtocolFactory, TcpConfig, WebSocketConfig};
#[cfg(feature = "quic")]
use crate::protocol::QuicConfig;
use crate::command::ConnectionInfo;
use crate::packet::Packet;
use crate::error::TransportError;
use crate::SessionId;
use super::tcp;
#[cfg(feature = "websocket")]
use super::websocket;
#[cfg(feature = "quic")]
use super::quic;

/// TCP适配器的Connection包装器
pub struct TcpConnection {
    inner: tcp::TcpAdapter,
}

impl TcpConnection {
    pub fn new(adapter: tcp::TcpAdapter) -> Self {
        Self { inner: adapter }
    }
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.send(packet).await.map_err(Into::into)
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.receive().await.map_err(Into::into)
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.close().await.map_err(Into::into)
    }
    
    fn is_connected(&self) -> bool {
        use crate::protocol::ProtocolAdapter;
        self.inner.is_connected()
    }
    
    fn session_id(&self) -> SessionId {
        use crate::protocol::ProtocolAdapter;
        self.inner.session_id()
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        use crate::protocol::ProtocolAdapter;
        self.inner.set_session_id(session_id);
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        use crate::protocol::ProtocolAdapter;
        self.inner.connection_info()
    }
}

/// TCP服务器的Server包装器
pub struct TcpServerWrapper {
    inner: tcp::TcpServer,
}

impl TcpServerWrapper {
    pub fn new(server: tcp::TcpServer) -> Self {
        Self { inner: server }
    }
}

#[async_trait]
impl Server for TcpServerWrapper {
    async fn accept(&mut self) -> Result<Box<dyn Connection>, TransportError> {
        tracing::debug!("TcpServerWrapper::accept - 等待新的TCP连接...");
        
        let adapter = self.inner.accept().await.map_err(|e| {
            tracing::error!("TcpServerWrapper::accept - TCP accept 错误: {:?}", e);
            TransportError::Connection(format!("TCP accept error: {:?}", e))
        })?;
        
        tracing::info!("TcpServerWrapper::accept - 成功接受新的TCP连接");
        Ok(Box::new(TcpConnection::new(adapter)))
    }
    
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError> {
        self.inner.local_addr().map_err(Into::into)
    }
    
    async fn shutdown(&mut self) -> Result<(), TransportError> {
        // TCP服务器没有shutdown方法，这里只是关闭监听
        Ok(())
    }
}

/// TCP协议工厂
pub struct TcpFactory;

impl TcpFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolFactory for TcpFactory {
    fn protocol_name(&self) -> &'static str {
        "tcp"
    }
    
    fn supported_schemes(&self) -> Vec<&'static str> {
        vec!["tcp", "tcp4", "tcp6"]
    }
    
    async fn create_connection(
        &self,
        uri: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Connection>, TransportError> {
        let (addr, _params) = self.parse_uri(uri)?;
        
        let tcp_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<TcpConfig>() {
                *config
            } else {
                TcpConfig::default()
            }
        } else {
            TcpConfig::default()
        };
        
        let adapter = tcp::TcpAdapter::connect(addr, tcp_config).await
            .map_err(|e| TransportError::Connection(format!("TCP connection failed: {:?}", e)))?;
        
        Ok(Box::new(TcpConnection::new(adapter)))
    }
    
    async fn create_server(
        &self,
        bind_addr: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Server>, TransportError> {
        let addr: std::net::SocketAddr = bind_addr.parse()
            .map_err(|_| TransportError::ProtocolConfiguration(format!("Invalid bind address: {}", bind_addr)))?;
        
        let tcp_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<TcpConfig>() {
                *config
            } else {
                TcpConfig::default()
            }
        } else {
            TcpConfig::default()
        };
        
        let server = tcp::TcpServerBuilder::new()
            .bind_address(addr)
            .config(tcp_config)
            .build()
            .await
            .map_err(|e| TransportError::ProtocolConfiguration(format!("TCP server creation failed: {:?}", e)))?;
        
        Ok(Box::new(TcpServerWrapper::new(server)))
    }
    
    fn default_config(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(TcpConfig::default())
    }
    
    fn parse_uri(&self, uri: &str) -> Result<(std::net::SocketAddr, HashMap<String, String>), TransportError> {
        // 处理 tcp://host:port 格式
        if let Some(stripped) = uri.strip_prefix("tcp://") {
            if let Ok(addr) = stripped.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::Configuration(format!("Invalid TCP URI: {}", uri)))
            }
        } else if let Some(stripped) = uri.strip_prefix("tcp4://") {
            if let Ok(addr) = stripped.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::Configuration(format!("Invalid TCP4 URI: {}", uri)))
            }
        } else if let Some(stripped) = uri.strip_prefix("tcp6://") {
            if let Ok(addr) = stripped.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::Configuration(format!("Invalid TCP6 URI: {}", uri)))
            }
        } else if let Ok(addr) = uri.parse::<std::net::SocketAddr>() {
            // 支持没有scheme的 host:port 格式
            Ok((addr, HashMap::new()))
        } else {
            Err(TransportError::Configuration(format!("Invalid TCP URI: {}", uri)))
        }
    }
}

#[cfg(feature = "websocket")]
/// WebSocket连接包装器
#[cfg(feature = "websocket")]
pub struct WebSocketConnection<S> {
    inner: websocket::WebSocketAdapter<S>,
}

#[cfg(feature = "websocket")]
#[cfg(feature = "websocket")]
impl<S> WebSocketConnection<S> {
    pub fn new(adapter: websocket::WebSocketAdapter<S>) -> Self {
        Self { inner: adapter }
    }
}

#[async_trait]
#[cfg(feature = "websocket")]
impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static> Connection for WebSocketConnection<S> {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.send(packet).await.map_err(Into::into)
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.receive().await.map_err(Into::into)
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.close().await.map_err(Into::into)
    }
    
    fn is_connected(&self) -> bool {
        use crate::protocol::ProtocolAdapter;
        self.inner.is_connected()
    }
    
    fn session_id(&self) -> SessionId {
        use crate::protocol::ProtocolAdapter;
        self.inner.session_id()
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        use crate::protocol::ProtocolAdapter;
        self.inner.set_session_id(session_id);
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        use crate::protocol::ProtocolAdapter;
        self.inner.connection_info()
    }
}

/// WebSocket服务器包装器
pub struct WebSocketServerWrapper {
    inner: websocket::WebSocketServer,
}

impl WebSocketServerWrapper {
    pub fn new(server: websocket::WebSocketServer) -> Self {
        Self { inner: server }
    }
}

#[async_trait]
impl Server for WebSocketServerWrapper {
    async fn accept(&mut self) -> Result<Box<dyn Connection>, TransportError> {
        let adapter = self.inner.accept().await.map_err(|e| TransportError::Connection(format!("WebSocket accept error: {:?}", e)))?;
        Ok(Box::new(WebSocketConnection::new(adapter)))
    }
    
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError> {
        self.inner.local_addr().map_err(Into::into)
    }
    
    async fn shutdown(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// WebSocket协议工厂
pub struct WebSocketFactory;

impl WebSocketFactory {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProtocolFactory for WebSocketFactory {
    fn protocol_name(&self) -> &'static str {
        "websocket"
    }
    
    fn supported_schemes(&self) -> Vec<&'static str> {
        vec!["ws", "wss", "websocket"]
    }
    
    async fn create_connection(
        &self,
        uri: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Connection>, TransportError> {
        let ws_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<WebSocketConfig>() {
                *config
            } else {
                WebSocketConfig::default()
            }
        } else {
            WebSocketConfig::default()
        };
        
        let adapter = websocket::WebSocketClientBuilder::new()
            .target_url(uri)
            .config(ws_config)
            .connect()
            .await
            .map_err(|e| TransportError::Connection(format!("WebSocket connection failed: {:?}", e)))?;
        
        Ok(Box::new(WebSocketConnection::new(adapter)))
    }
    
    async fn create_server(
        &self,
        bind_addr: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Server>, TransportError> {
        let addr: std::net::SocketAddr = bind_addr.parse()
            .map_err(|_| TransportError::ProtocolConfiguration(format!("Invalid bind address: {}", bind_addr)))?;
        
        let ws_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<WebSocketConfig>() {
                *config
            } else {
                WebSocketConfig::default()
            }
        } else {
            WebSocketConfig::default()
        };
        
        let server = websocket::WebSocketServerBuilder::new()
            .bind_address(addr)
            .config(ws_config)
            .build()
            .await
            .map_err(|e| TransportError::ProtocolConfiguration(format!("WebSocket server creation failed: {:?}", e)))?;
        
        Ok(Box::new(WebSocketServerWrapper::new(server)))
    }
    
    fn default_config(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(WebSocketConfig::default())
    }
    
    fn parse_uri(&self, uri: &str) -> Result<(std::net::SocketAddr, HashMap<String, String>), TransportError> {
        // 简化的WebSocket URI解析
        if let Some(url) = uri.strip_prefix("ws://") {
            if let Ok(addr) = url.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::ProtocolConfiguration(format!("Invalid WebSocket URI: {}", uri)))
            }
        } else if let Some(url) = uri.strip_prefix("wss://") {
            if let Ok(addr) = url.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::ProtocolConfiguration(format!("Invalid WebSocket URI: {}", uri)))
            }
        } else {
            Err(TransportError::ProtocolConfiguration(format!("Unsupported WebSocket scheme: {}", uri)))
        }
    }
}

#[cfg(feature = "quic")]
/// QUIC连接包装器
pub struct QuicConnection {
    inner: quic::QuicAdapter,
}

#[cfg(feature = "quic")]
impl QuicConnection {
    pub fn new(adapter: quic::QuicAdapter) -> Self {
        Self { inner: adapter }
    }
}

#[cfg(feature = "quic")]
#[async_trait]
impl Connection for QuicConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.send(packet).await.map_err(Into::into)
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.receive().await.map_err(Into::into)
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.inner.close().await.map_err(Into::into)
    }
    
    fn is_connected(&self) -> bool {
        use crate::protocol::ProtocolAdapter;
        self.inner.is_connected()
    }
    
    fn session_id(&self) -> SessionId {
        use crate::protocol::ProtocolAdapter;
        self.inner.session_id()
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        use crate::protocol::ProtocolAdapter;
        self.inner.set_session_id(session_id);
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        use crate::protocol::ProtocolAdapter;
        self.inner.connection_info()
    }
}

#[cfg(feature = "quic")]
/// QUIC服务器包装器
pub struct QuicServerWrapper {
    inner: quic::QuicServer,
}

#[cfg(feature = "quic")]
impl QuicServerWrapper {
    pub fn new(server: quic::QuicServer) -> Self {
        Self { inner: server }
    }
}

#[cfg(feature = "quic")]
#[async_trait]
impl Server for QuicServerWrapper {
    async fn accept(&mut self) -> Result<Box<dyn Connection>, TransportError> {
        let adapter = self.inner.accept().await.map_err(|e| TransportError::Connection(format!("QUIC accept error: {:?}", e)))?;
        Ok(Box::new(QuicConnection::new(adapter)))
    }
    
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError> {
        self.inner.local_addr().map_err(|e| TransportError::Connection(format!("Failed to get local addr: {:?}", e)))
    }
    
    async fn shutdown(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

#[cfg(feature = "quic")]
/// QUIC协议工厂
pub struct QuicFactory;

#[cfg(feature = "quic")]
impl QuicFactory {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(feature = "quic")]
#[async_trait]
impl ProtocolFactory for QuicFactory {
    fn protocol_name(&self) -> &'static str {
        "quic"
    }
    
    fn supported_schemes(&self) -> Vec<&'static str> {
        vec!["quic", "quic+udp"]
    }
    
    async fn create_connection(
        &self,
        uri: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Connection>, TransportError> {
        let (addr, _params) = self.parse_uri(uri)?;
        
        let quic_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<QuicConfig>() {
                *config
            } else {
                QuicConfig::default()
            }
        } else {
            QuicConfig::default()
        };
        
        let adapter = quic::QuicAdapter::connect(addr, quic_config).await
            .map_err(|e| TransportError::Connection(format!("QUIC connection failed: {:?}", e)))?;
        
        Ok(Box::new(QuicConnection::new(adapter)))
    }
    
    async fn create_server(
        &self,
        bind_addr: &str,
        config: Option<Box<dyn Any + Send + Sync>>
    ) -> Result<Box<dyn Server>, TransportError> {
        let addr: std::net::SocketAddr = bind_addr.parse()
            .map_err(|_| TransportError::ProtocolConfiguration(format!("Invalid bind address: {}", bind_addr)))?;
        
        let quic_config = if let Some(config_box) = config {
            if let Ok(config) = config_box.downcast::<QuicConfig>() {
                *config
            } else {
                QuicConfig::default()
            }
        } else {
            QuicConfig::default()
        };
        
        let server = quic::QuicServerBuilder::new()
            .bind_address(addr)
            .config(quic_config)
            .build()
            .await
            .map_err(|e| TransportError::ProtocolConfiguration(format!("QUIC server creation failed: {:?}", e)))?;
        
        Ok(Box::new(QuicServerWrapper::new(server)))
    }
    
    fn default_config(&self) -> Box<dyn Any + Send + Sync> {
        Box::new(QuicConfig::default())
    }
    
    fn parse_uri(&self, uri: &str) -> Result<(std::net::SocketAddr, HashMap<String, String>), TransportError> {
        if let Some(url) = uri.strip_prefix("quic://") {
            if let Ok(addr) = url.parse::<std::net::SocketAddr>() {
                Ok((addr, HashMap::new()))
            } else {
                Err(TransportError::ProtocolConfiguration(format!("Invalid QUIC URI: {}", uri)))
            }
        } else {
            // 默认解析为 host:port
            self.parse_uri(&format!("quic://{}", uri))
        }
    }
}

/// 便利函数：创建标准协议注册表
pub async fn create_standard_registry() -> Result<crate::protocol::ProtocolRegistry, TransportError> {
    let registry = crate::protocol::ProtocolRegistry::new();
    
    // 注册标准协议
    registry.register(TcpFactory::new()).await?;
    
    #[cfg(feature = "websocket")]
    registry.register(WebSocketFactory::new()).await?;
    
    #[cfg(feature = "quic")]
    registry.register(QuicFactory::new()).await?;
    
    Ok(registry)
} 