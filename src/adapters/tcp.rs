use async_trait::async_trait;
use tokio::net::{TcpStream, TcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io;
use crate::{
    SessionId, 
    packet::{Packet, PacketError},
    protocol::{ProtocolAdapter, AdapterStats, TcpClientConfig, TcpServerConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
};

/// TCP适配器错误类型
#[derive(Debug, thiserror::Error)]
pub enum TcpError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Connection timeout")]
    Timeout,
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Packet error: {0}")]
    Packet(#[from] PacketError),
    
    #[error("Buffer overflow")]
    BufferOverflow,
    
    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<TcpError> for TransportError {
    fn from(error: TcpError) -> Self {
        match error {
            TcpError::Io(io_err) => TransportError::connection_error(format!("TCP IO error: {:?}", io_err), true),
            TcpError::Timeout => TransportError::connection_error("TCP connection timeout", true),
            TcpError::ConnectionClosed => TransportError::connection_error("TCP connection closed", true),
            TcpError::Packet(packet_err) => TransportError::protocol_error("packet", format!("TCP packet error: {}", packet_err)),
            TcpError::BufferOverflow => TransportError::protocol_error("generic", "TCP buffer overflow".to_string()),
            TcpError::Config(msg) => TransportError::config_error("tcp", msg),
        }
    }
}

/// TCP协议适配器（泛型支持客户端和服务端配置）
pub struct TcpAdapter<C> {
    /// TCP流
    stream: TcpStream,
    /// 会话ID
    session_id: SessionId,
    /// 配置
    config: C,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// 连接状态
    is_connected: bool,
}

impl<C> TcpAdapter<C> {
    /// 创建新的TCP适配器
    pub async fn new(stream: TcpStream, config: C) -> Result<Self, TcpError> {
        // 设置基本TCP选项
        stream.set_nodelay(true)?;
        
        let local_addr = stream.local_addr()?;
        let peer_addr = stream.peer_addr()?;
        
        let mut connection_info = ConnectionInfo::default();
        connection_info.local_addr = local_addr;
        connection_info.peer_addr = peer_addr;
        connection_info.protocol = ProtocolType::Tcp;
        connection_info.state = ConnectionState::Connected;
        connection_info.established_at = std::time::SystemTime::now();
        
        Ok(Self {
            stream,
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection_info,
            is_connected: true,
        })
    }
    
    /// 读取完整的数据包
    async fn read_packet(&mut self) -> Result<Option<Packet>, TcpError> {
        // 首先读取包头（9字节）
        let mut header_buf = [0u8; 9];
        match self.stream.read_exact(&mut header_buf).await {
            Ok(_) => {},
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                self.is_connected = false;
                self.connection_info.state = ConnectionState::Closed;
                self.connection_info.closed_at = Some(std::time::SystemTime::now());
                return Ok(None);
            }
            Err(e) => return Err(TcpError::Io(e)),
        }
        
        // 解析包头获取负载长度
        let payload_len = u32::from_be_bytes([header_buf[5], header_buf[6], header_buf[7], header_buf[8]]) as usize;
        
        // 防止恶意的大数据包
        if payload_len > 1024 * 1024 { // 1MB 限制
            return Err(TcpError::BufferOverflow);
        }
        
        // 读取负载
        let mut payload = vec![0u8; payload_len];
        self.stream.read_exact(&mut payload).await?;
        
        // 重构完整的数据包
        let mut packet_data = Vec::with_capacity(9 + payload_len);
        packet_data.extend_from_slice(&header_buf);
        packet_data.extend_from_slice(&payload);
        
        // 解析数据包
        let packet = Packet::from_bytes(&packet_data)?;
        Ok(Some(packet))
    }
}

// 客户端适配器实现
impl TcpAdapter<TcpClientConfig> {
    /// 连接到TCP服务器
    pub async fn connect(addr: std::net::SocketAddr, config: TcpClientConfig) -> Result<Self, TcpError> {
        tracing::debug!("🔌 TCP客户端连接到: {}", addr);
        
        let stream = if config.connect_timeout != std::time::Duration::from_secs(0) {
            tokio::time::timeout(config.connect_timeout, TcpStream::connect(addr))
                .await
                .map_err(|_| TcpError::Timeout)?
                .map_err(TcpError::Io)?
        } else {
            TcpStream::connect(addr).await.map_err(TcpError::Io)?
        };
        
        tracing::debug!("✅ TCP连接建立成功");
        
        Self::new(stream, config).await
    }
}

#[async_trait]
impl ProtocolAdapter for TcpAdapter<TcpClientConfig> {
    type Config = TcpClientConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(TcpError::ConnectionClosed);
        }
        
        let data = packet.to_bytes();
        
        // 应用写超时
        let write_future = self.stream.write_all(&data);
        
        if let Some(timeout) = self.config.write_timeout {
            tokio::time::timeout(timeout, write_future).await
                .map_err(|_| TcpError::Timeout)?
                .map_err(TcpError::Io)?;
        } else {
            write_future.await.map_err(TcpError::Io)?;
        }
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }

        // 应用读超时
        if let Some(timeout) = self.config.read_timeout {
            let read_future = self.read_packet();
            tokio::time::timeout(timeout, read_future).await
                .map_err(|_| TcpError::Timeout)?
        } else {
            self.read_packet().await
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            let _ = self.stream.shutdown().await;
            self.is_connected = false;
            self.connection_info.state = ConnectionState::Closed;
            self.connection_info.closed_at = Some(std::time::SystemTime::now());
        }
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id = session_id;
        self.connection_info.session_id = session_id;
    }
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 尝试读取但不消费数据
        let mut buf = [0u8; 1];
        match self.stream.try_read(&mut buf) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(_) => Ok(false),
        }
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.flush().await?;
        Ok(())
    }
}

// 服务端适配器实现
#[async_trait]
impl ProtocolAdapter for TcpAdapter<TcpServerConfig> {
    type Config = TcpServerConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(TcpError::ConnectionClosed);
        }
        
        let data = packet.to_bytes();
        self.stream.write_all(&data).await?;
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }

        self.read_packet().await
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            let _ = self.stream.shutdown().await;
            self.is_connected = false;
            self.connection_info.state = ConnectionState::Closed;
            self.connection_info.closed_at = Some(std::time::SystemTime::now());
        }
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id = session_id;
        self.connection_info.session_id = session_id;
    }
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        let mut buf = [0u8; 1];
        match self.stream.try_read(&mut buf) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(false),
            Err(_) => Ok(false),
        }
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.flush().await?;
        Ok(())
    }
}

/// TCP服务器构建器
pub(crate) struct TcpServerBuilder {
    config: TcpServerConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl TcpServerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: TcpServerConfig::default(),
            bind_address: None,
        }
    }
    
    pub(crate) fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    pub(crate) fn config(mut self, config: TcpServerConfig) -> Self {
        self.config = config;
        self
    }
    
    pub(crate) async fn build(self) -> Result<TcpServer, TcpError> {
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        
        tracing::debug!("🚀 TCP服务器启动在: {}", bind_addr);
        
        let listener = TcpListener::bind(bind_addr).await?;
        
        tracing::info!("✅ TCP服务器成功启动在: {}", listener.local_addr()?);
        
        Ok(TcpServer {
            listener,
            config: self.config,
        })
    }
}

impl Default for TcpServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// TCP服务器
pub(crate) struct TcpServer {
    listener: TcpListener,
    config: TcpServerConfig,
}

impl TcpServer {
    pub(crate) fn builder() -> TcpServerBuilder {
        TcpServerBuilder::new()
    }
    
    pub(crate) async fn accept(&mut self) -> Result<TcpAdapter<TcpServerConfig>, TcpError> {
        let (stream, peer_addr) = self.listener.accept().await?;
        
        tracing::debug!("🔗 TCP新连接来自: {}", peer_addr);
        
        TcpAdapter::new(stream, self.config.clone()).await
    }
    
    pub(crate) fn local_addr(&self) -> Result<std::net::SocketAddr, TcpError> {
        Ok(self.listener.local_addr()?)
    }
}

/// TCP客户端构建器
pub(crate) struct TcpClientBuilder {
    config: TcpClientConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl TcpClientBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: TcpClientConfig::default(),
            target_address: None,
        }
    }
    
    pub(crate) fn target_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_address = Some(addr);
        self
    }
    
    pub(crate) fn config(mut self, config: TcpClientConfig) -> Self {
        self.config = config;
        self
    }
    
    pub(crate) async fn connect(self) -> Result<TcpAdapter<TcpClientConfig>, TcpError> {
        let target_addr = self.target_address.unwrap_or(self.config.target_address);
        TcpAdapter::connect(target_addr, self.config).await
    }
}

impl Default for TcpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
} 