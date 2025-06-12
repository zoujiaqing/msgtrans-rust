use async_trait::async_trait;
use crate::{SessionId, packet::Packet, error::TransportError, command::ConnectionInfo};

/// 连接接口 - 统一的连接抽象
#[async_trait]
pub trait Connection: Send + Sync {
    /// 发送数据包
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError>;
    
    /// 接收数据包
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError>;
    
    /// 关闭连接
    async fn close(&mut self) -> Result<(), TransportError>;
    
    /// 获取会话ID
    fn session_id(&self) -> SessionId;
    
    /// 获取连接信息
    fn info(&self) -> &ConnectionInfo;
    
    /// 检查连接是否活跃
    fn is_active(&self) -> bool;
    
    /// 刷新缓冲区
    async fn flush(&mut self) -> Result<(), TransportError>;
    
    /// 获取事件流 - TCP连接特有的实现
    fn get_event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>>;
}

/// 服务器接口 - 接受新连接
#[async_trait]
pub trait Server: Send + Sync {
    type Connection: Connection;
    
    /// 接受新连接
    async fn accept(&mut self) -> Result<Self::Connection, TransportError>;
    
    /// 获取服务器绑定地址
    fn local_addr(&self) -> Result<std::net::SocketAddr, TransportError>;
    
    /// 关闭服务器
    async fn shutdown(&mut self) -> Result<(), TransportError>;
}

/// 连接工厂 - 创建客户端连接
#[async_trait]
pub trait ConnectionFactory: Send + Sync {
    type Connection: Connection;
    
    /// 建立连接
    async fn connect(&self) -> Result<Self::Connection, TransportError>;
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
    
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.receive().await.map_err(Into::into)
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.close().await.map_err(Into::into)
    }
    
    fn session_id(&self) -> SessionId {
        use crate::protocol::ProtocolAdapter;
        self.adapter.session_id()
    }
    
    fn info(&self) -> &ConnectionInfo {
        &self.cached_info
    }
    
    fn is_active(&self) -> bool {
        use crate::protocol::ProtocolAdapter;
        self.adapter.is_connected()
    }
    
    async fn flush(&mut self) -> Result<(), TransportError> {
        use crate::protocol::ProtocolAdapter;
        self.adapter.flush().await.map_err(Into::into)
    }
    
    /// 获取事件流 - TCP连接特有的实现
    fn get_event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
        Some(self.adapter.subscribe_events())
    }
}

// 为 protocol::protocol::Connection trait 添加实现
#[async_trait]
impl crate::protocol::Connection for TcpConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        <Self as Connection>::send(self, packet).await
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, TransportError> {
        <Self as Connection>::receive(self).await
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        <Self as Connection>::close(self).await
    }
    
    fn is_connected(&self) -> bool {
        self.is_active()
    }
    
    fn session_id(&self) -> SessionId {
        <Self as Connection>::session_id(self)
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        use crate::protocol::ProtocolAdapter;
        self.adapter.set_session_id(session_id);
    }
    
    fn connection_info(&self) -> crate::command::ConnectionInfo {
        self.info().clone()
    }
    
    /// 获取事件流 - TCP连接特有的实现
    fn get_event_stream(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
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
    type Connection = TcpConnection;
    
    async fn accept(&mut self) -> Result<Self::Connection, TransportError> {
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
    type Connection = TcpConnection;
    
    async fn connect(&self) -> Result<Self::Connection, TransportError> {
        let adapter = crate::adapters::tcp::TcpAdapter::connect(self.target_addr, self.config.clone())
            .await
            .map_err(|e| TransportError::protocol_error("generic", format!("TCP connect failed: {:?}", e)))?;
        
        Ok(TcpConnection::new(adapter))
    }
} 