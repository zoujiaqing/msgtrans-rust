use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::io;
use crate::{
    SessionId, 
    packet::{UnifiedPacket, PacketError},
    protocol::{ProtocolAdapter, AdapterStats, TcpConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
};

/// TCP适配器错误类型
#[derive(Debug, thiserror::Error)]
pub enum TcpError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Packet error: {0}")]
    Packet(#[from] PacketError),
    
    #[error("Buffer overflow")]
    BufferOverflow,
}

impl From<TcpError> for TransportError {
    fn from(error: TcpError) -> Self {
        match error {
            TcpError::Io(io_err) => TransportError::Io(io_err),
            TcpError::ConnectionClosed => TransportError::Connection("Connection closed".to_string()),
            TcpError::Packet(p_err) => TransportError::Protocol(format!("Packet error: {}", p_err)),
            TcpError::BufferOverflow => TransportError::Protocol("Buffer overflow".to_string()),
        }
    }
}

/// TCP协议适配器
/// 
/// 实现了TCP连接的发送和接收功能
pub struct TcpAdapter {
    /// TCP流
    stream: TcpStream,
    /// 会话ID
    session_id: SessionId,
    /// 配置
    config: TcpConfig,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// 连接状态
    is_connected: bool,
}

impl TcpAdapter {
    /// 创建新的TCP适配器
    pub async fn new(stream: TcpStream, config: TcpConfig) -> Result<Self, TcpError> {
        // 设置TCP选项
        stream.set_nodelay(config.nodelay)?;
        
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
            session_id: 0, // 将由调用者设置
            config,
            stats: AdapterStats::new(),
            connection_info,
            is_connected: true,
        })
    }
    
    /// 从地址创建TCP连接
    pub async fn connect(addr: std::net::SocketAddr, config: TcpConfig) -> Result<Self, TcpError> {
        let stream = tokio::time::timeout(
            config.connect_timeout,
            TcpStream::connect(addr)
        ).await
        .map_err(|_| TcpError::Io(io::Error::new(io::ErrorKind::TimedOut, "Connection timeout")))?
        .map_err(TcpError::Io)?;
        
        Self::new(stream, config).await
    }
    
    /// 读取完整的数据包
    async fn read_packet(&mut self) -> Result<Option<UnifiedPacket>, TcpError> {
        tracing::debug!("TCP 适配器开始读取数据包 (session {})", self.session_id);
        
        // 首先读取包头（9字节）
        let mut header_buf = [0u8; 9];
        match self.stream.read_exact(&mut header_buf).await {
            Ok(_) => {
                tracing::debug!("TCP 成功读取包头: {:?}", header_buf);
            },
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                tracing::debug!("TCP 连接被对端关闭 (session {})", self.session_id);
                self.is_connected = false;
                return Ok(None);
            }
            Err(e) => {
                tracing::error!("TCP 读取包头失败 (session {}): {:?}", self.session_id, e);
                return Err(TcpError::Io(e));
            }
        }
        
        // 解析包头获取负载长度
        let payload_len = u32::from_be_bytes([header_buf[5], header_buf[6], header_buf[7], header_buf[8]]) as usize;
        tracing::debug!("TCP 解析负载长度: {} bytes (session {})", payload_len, self.session_id);
        
        // 防止恶意的大数据包
        if payload_len > self.config.read_buffer_size {
            tracing::error!("TCP 负载过大: {} > {} (session {})", payload_len, self.config.read_buffer_size, self.session_id);
            return Err(TcpError::BufferOverflow);
        }
        
        // 读取负载
        let mut payload = vec![0u8; payload_len];
        match self.stream.read_exact(&mut payload).await {
            Ok(_) => {
                tracing::debug!("TCP 成功读取负载: {} bytes (session {})", payload_len, self.session_id);
            }
            Err(e) => {
                tracing::error!("TCP 读取负载失败 (session {}): {:?}", self.session_id, e);
                return Err(TcpError::Io(e));
            }
        }
        
        // 重构完整的数据包
        let mut packet_data = Vec::with_capacity(9 + payload_len);
        packet_data.extend_from_slice(&header_buf);
        packet_data.extend_from_slice(&payload);
        
        tracing::debug!("TCP 重构完整数据包: {} bytes (session {})", packet_data.len(), self.session_id);
        
        // 解析数据包
        match UnifiedPacket::from_bytes(&packet_data) {
            Ok(packet) => {
                tracing::debug!("TCP 数据包解析成功: 类型{:?}, ID{} (session {})", 
                              packet.packet_type, packet.message_id, self.session_id);
                Ok(Some(packet))
            },
            Err(e) => {
                tracing::error!("TCP 数据包解析失败 (session {}): {:?}", self.session_id, e);
                tracing::error!("原始数据: {:?}", packet_data);
                Err(TcpError::Packet(e))
            }
        }
    }
}

#[async_trait]
impl ProtocolAdapter for TcpAdapter {
    type Config = TcpConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: UnifiedPacket) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(TcpError::ConnectionClosed);
        }
        
        let data = packet.to_bytes();
        
        // 应用写超时
        let write_future = self.stream.write_all(&data);
        
        if let Some(timeout) = self.config.write_timeout {
            tokio::time::timeout(timeout, write_future).await
                .map_err(|_| TcpError::Io(io::Error::new(io::ErrorKind::TimedOut, "Write timeout")))?
                .map_err(TcpError::Io)?;
        } else {
            write_future.await.map_err(TcpError::Io)?;
        }
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
        async fn receive(&mut self) -> Result<Option<UnifiedPacket>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }

        // 提前获取超时配置以避免借用冲突
        let read_timeout = self.config.read_timeout;
        
        // 应用读超时
        let result = if let Some(timeout) = read_timeout {
            tokio::time::timeout(timeout, self.read_packet()).await
                .map_err(|_| TcpError::Io(io::Error::new(io::ErrorKind::TimedOut, "Read timeout")))?
        } else {
            self.read_packet().await
        };
        
        match result {
            Ok(Some(packet)) => {
                // 记录统计信息
                let packet_size = packet.to_bytes().len();
                self.stats.record_packet_received(packet_size);
                self.connection_info.record_packet_received(packet_size);
                Ok(Some(packet))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                self.stats.record_error();
                Err(e)
            }
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            self.stream.shutdown().await.map_err(TcpError::Io)?;
            self.is_connected = false;
            self.connection_info.state = ConnectionState::Closed;
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
        // 使用stream的ready API检查是否可读
        match self.stream.ready(tokio::io::Interest::READABLE).await {
            Ok(_) => Ok(true),
            Err(e) => Err(TcpError::Io(e)),
        }
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.flush().await.map_err(TcpError::Io)
    }
}

/// TCP服务器构建器
pub struct TcpServerBuilder {
    config: TcpConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl TcpServerBuilder {
    /// 创建新的TCP服务器构建器
    pub fn new() -> Self {
        Self {
            config: TcpConfig::default(),
            bind_address: None,
        }
    }
    
    /// 设置绑定地址
    pub fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self.config.bind_address = addr;
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: TcpConfig) -> Self {
        if let Some(addr) = self.bind_address {
            self.config = config;
            self.config.bind_address = addr;
        } else {
            self.config = config;
        }
        self
    }
    
    /// 启动TCP服务器
    pub async fn build(self) -> Result<TcpServer, TcpError> {
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        let listener = tokio::net::TcpListener::bind(bind_addr).await.map_err(TcpError::Io)?;
        
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
pub struct TcpServer {
    listener: tokio::net::TcpListener,
    config: TcpConfig,
}

impl TcpServer {
    /// 创建服务器构建器
    pub fn builder() -> TcpServerBuilder {
        TcpServerBuilder::new()
    }
    
    /// 接受新连接
    pub async fn accept(&mut self) -> Result<TcpAdapter, TcpError> {
        let (stream, _) = self.listener.accept().await.map_err(TcpError::Io)?;
        TcpAdapter::new(stream, self.config.clone()).await
    }
    
    /// 获取本地地址
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, TcpError> {
        self.listener.local_addr().map_err(TcpError::Io)
    }
}

/// TCP客户端构建器
pub struct TcpClientBuilder {
    config: TcpConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl TcpClientBuilder {
    /// 创建新的TCP客户端构建器
    pub fn new() -> Self {
        Self {
            config: TcpConfig::default(),
            target_address: None,
        }
    }
    
    /// 设置目标地址
    pub fn target_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_address = Some(addr);
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: TcpConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 连接到服务器
    pub async fn connect(self) -> Result<TcpAdapter, TcpError> {
        let target_addr = self.target_address
            .ok_or_else(|| TcpError::Io(io::Error::new(io::ErrorKind::InvalidInput, "Target address not set")))?;
        
        TcpAdapter::connect(target_addr, self.config).await
    }
}

impl Default for TcpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
} 