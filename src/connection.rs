use async_trait::async_trait;
use crate::{SessionId, packet::Packet, error::TransportError, command::ConnectionInfo};

/// 统一的连接接口 - 所有协议的连接抽象
/// 
/// 这是 msgtrans 中唯一的连接接口，所有协议适配器都应该实现此接口
#[async_trait]
pub trait Connection: Send + Sync + std::any::Any {
    /// 发送数据包
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError>;
    
    /// 关闭连接
    async fn close(&mut self) -> Result<(), TransportError>;
    
    /// 获取会话ID
    fn session_id(&self) -> SessionId;
    
    /// 设置会话ID
    fn set_session_id(&mut self, session_id: SessionId);
    
    /// 获取连接信息
    fn connection_info(&self) -> ConnectionInfo;
    
    /// 检查连接是否活跃
    fn is_connected(&self) -> bool;
    
    /// 刷新发送缓冲区
    async fn flush(&mut self) -> Result<(), TransportError>;
    
    /// 获取事件流 - 事件驱动架构的核心
    /// 
    /// 所有连接都应该支持事件流，这是事件驱动架构的基础
    fn event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>>;
}

/// 统一的服务器接口 - 接受新连接
#[async_trait]
pub trait Server: Send + Sync {
    /// 接受新连接
    async fn accept(&mut self) -> Result<Box<dyn Connection>, TransportError>;
    
    /// 获取服务器绑定地址
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError>;
    
    /// 关闭服务器
    async fn shutdown(&mut self) -> Result<(), TransportError>;
}

/// 连接工厂 - 创建客户端连接
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    /// 建立连接
    async fn connect(&self) -> Result<Box<dyn Connection>, TransportError>;
}

/// TCP连接包装器
pub struct TcpConnection {
    adapter: crate::adapters::tcp::TcpAdapter<crate::protocol::TcpClientConfig>,
    cached_info: ConnectionInfo,
}

impl TcpConnection {
    pub fn new(adapter: crate::adapters::tcp::TcpAdapter<crate::protocol::TcpClientConfig>) -> Self {
        use crate::protocol::ProtocolAdapter;
        let cached_info = adapter.connection_info();
        Self { adapter, cached_info }
    }
    
    /// 获取事件流接收器
    /// 
    /// 这允许客户端订阅TCP适配器内部事件循环发送的事件
    pub fn subscribe_events(&self) -> tokio::sync::broadcast::Receiver<crate::event::TransportEvent> {
        self.adapter.subscribe_events()
    }
}

#[async_trait]
impl Connection for TcpConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.send(packet).await.map_err(Into::into)
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.close().await.map_err(Into::into)
    }
    
    fn session_id(&self) -> SessionId {
        use crate::protocol::ProtocolAdapter;
        self.adapter.session_id()
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        use crate::protocol::ProtocolAdapter;
        self.adapter.set_session_id(session_id);
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.cached_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        use crate::protocol::ProtocolAdapter;
        self.adapter.is_connected()
    }
    
    async fn flush(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.flush().await.map_err(Into::into)
    }
    
    /// 获取事件流 - TCP连接特有的实现
    fn event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
        Some(self.adapter.subscribe_events())
    }
}

/// TCP服务器包装器
pub struct TcpServer {
    inner: crate::adapters::tcp::TcpServer,
}

impl TcpServer {
    pub fn new(inner: crate::adapters::tcp::TcpServer) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Server for TcpServer {
    async fn accept(&mut self) -> Result<Box<dyn Connection>, TransportError> {
        let adapter = self.inner.accept().await.map_err(|e| {
            TransportError::protocol_error("generic", format!("TCP accept failed: {:?}", e))
        })?;
        
        // 注意：这里需要将服务器端的adapter转换为客户端连接
        // TODO: 需要实现适当的转换逻辑
        Err(TransportError::config_error("tcp", "TCP server accept not fully implemented yet"))
    }
    
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError> {
        self.inner.local_addr().map_err(|e| {
            TransportError::protocol_error("generic", format!("Failed to get local address: {:?}", e))
        })
    }
    
    async fn shutdown(&mut self) -> Result<(), TransportError> {
        // TCP服务器没有显式的shutdown方法，这里只是标记
        Ok(())
    }
}

/// TCP连接工厂
pub struct TcpConnectionFactory {
    target_addr: std::net::SocketAddr,
    config: crate::protocol::TcpClientConfig,
}

impl TcpConnectionFactory {
    pub fn new(target_addr: std::net::SocketAddr, config: crate::protocol::TcpClientConfig) -> Self {
        Self { target_addr, config }
    }
}

#[async_trait]
impl ConnectionFactory for TcpConnectionFactory {
    async fn connect(&self) -> Result<Box<dyn Connection>, TransportError> {
        let adapter = crate::adapters::tcp::TcpAdapter::connect(self.target_addr, self.config.clone())
            .await
            .map_err(|e| TransportError::protocol_error("generic", format!("TCP connect failed: {:?}", e)))?;
            
        Ok(Box::new(TcpConnection::new(adapter)))
    }
} 