use async_trait::async_trait;
use std::io;
use bytes::BytesMut;
use super::super::{
    SessionId, 
    adapter::{ProtocolAdapter, AdapterStats, QuicConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
    packet::{UnifiedPacket, PacketType},
};

/// QUIC适配器错误类型
#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    #[error("QUIC connection error: {0}")]
    Connection(String),
    
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Stream error: {0}")]
    Stream(String),
    
    #[error("Certificate error: {0}")]
    Certificate(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<QuicError> for TransportError {
    fn from(error: QuicError) -> Self {
        match error {
            QuicError::Connection(msg) => TransportError::Connection(msg),
            QuicError::Io(io_err) => TransportError::Io(io_err),
            QuicError::ConnectionClosed => TransportError::Connection("Connection closed".to_string()),
            QuicError::Stream(msg) => TransportError::Protocol(format!("Stream error: {}", msg)),
            QuicError::Certificate(msg) => TransportError::Authentication(msg),
            QuicError::Serialization(msg) => TransportError::Serialization(msg),
        }
    }
}

/// QUIC协议适配器
/// 
/// 实现了QUIC连接的发送和接收功能
/// 注意：这是一个简化的实现，实际的QUIC适配器需要更复杂的流管理
pub struct QuicAdapter {
    /// 会话ID
    session_id: SessionId,
    /// 配置
    config: QuicConfig,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// 连接状态
    is_connected: bool,
    /// 模拟的数据缓冲区（实际实现中应该是QUIC流）
    receive_buffer: Vec<UnifiedPacket>,
    /// 发送缓冲区
    send_buffer: Vec<UnifiedPacket>,
}

impl QuicAdapter {
    /// 创建新的QUIC适配器
    pub fn new(
        config: QuicConfig,
        local_addr: std::net::SocketAddr,
        peer_addr: std::net::SocketAddr,
    ) -> Self {
        let mut connection_info = ConnectionInfo::default();
        connection_info.local_addr = local_addr;
        connection_info.peer_addr = peer_addr;
        connection_info.protocol = ProtocolType::Quic;
        connection_info.state = ConnectionState::Connected;
        connection_info.established_at = std::time::SystemTime::now();
        
        Self {
            session_id: 0,
            config,
            stats: AdapterStats::new(),
            connection_info,
            is_connected: true,
            receive_buffer: Vec::new(),
            send_buffer: Vec::new(),
        }
    }
    
    /// 模拟连接到QUIC服务器
    pub async fn connect(
        addr: std::net::SocketAddr,
        config: QuicConfig,
    ) -> Result<Self, QuicError> {
        // 在实际实现中，这里会使用quinn或类似的QUIC库进行连接
        // 目前我们返回一个模拟的连接
        
        let local_addr = "0.0.0.0:0".parse().unwrap();
        
        // 模拟连接延迟
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        
        Ok(Self::new(config, local_addr, addr))
    }
    
    /// 序列化数据包
    fn serialize_packet(&self, packet: &UnifiedPacket) -> Result<Vec<u8>, QuicError> {
        // 简单的序列化格式：[长度:4字节][类型:1字节][负载]
        let mut buffer = Vec::new();
        let payload_len = packet.payload.len();
        
        if payload_len > u32::MAX as usize {
            return Err(QuicError::Serialization("Payload too large".to_string()));
        }
        
        buffer.extend_from_slice(&(payload_len as u32).to_be_bytes());
        buffer.push(packet.packet_type.into());
        buffer.extend_from_slice(&packet.payload);
        
        Ok(buffer)
    }
    
    /// 反序列化数据包
    fn deserialize_packet(&self, data: &[u8]) -> Result<UnifiedPacket, QuicError> {
        if data.len() < 5 {
            return Err(QuicError::Serialization("Data too short".to_string()));
        }
        
        let payload_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        
        if data.len() != payload_len + 5 {
            return Err(QuicError::Serialization("Invalid data length".to_string()));
        }
        
        let packet_type = data[4];
        let payload = data[5..].to_vec();
        
        Ok(UnifiedPacket {
            packet_type: PacketType::from(packet_type),
            message_id: 0,
            payload: BytesMut::from(&payload[..]),
        })
    }
}

#[async_trait]
impl ProtocolAdapter for QuicAdapter {
    type Config = QuicConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: UnifiedPacket) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(QuicError::ConnectionClosed);
        }
        
        // 在实际实现中，这里会通过QUIC流发送数据
        // 目前我们模拟发送过程
        let data = self.serialize_packet(&packet)?;
        self.send_buffer.push(packet.clone());
        
        // 模拟网络延迟
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<UnifiedPacket>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }
        
        // 在实际实现中，这里会从QUIC流接收数据
        // 目前我们从缓冲区模拟接收
        
        if let Some(packet) = self.receive_buffer.pop() {
            let packet_size = packet.payload.len() + 5; // +5 for header
            
            // 记录统计信息
            self.stats.record_packet_received(packet_size);
            self.connection_info.record_packet_received(packet_size);
            
            Ok(Some(packet))
        } else {
            // 模拟没有数据的情况
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            Ok(None)
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            // 在实际实现中，这里会关闭QUIC连接
            self.is_connected = false;
            self.connection_info.state = ConnectionState::Closed;
            
            // 清理缓冲区
            self.receive_buffer.clear();
            self.send_buffer.clear();
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
        // 检查是否有数据可读
        Ok(!self.receive_buffer.is_empty() && self.is_connected)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 在实际实现中，这里会刷新QUIC流
        // 目前我们清空发送缓冲区来模拟
        self.send_buffer.clear();
        Ok(())
    }
}

/// QUIC服务器构建器
pub struct QuicServerBuilder {
    config: QuicConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl QuicServerBuilder {
    /// 创建新的QUIC服务器构建器
    pub fn new() -> Self {
        Self {
            config: QuicConfig::default(),
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
    pub fn config(mut self, config: QuicConfig) -> Self {
        if let Some(addr) = self.bind_address {
            self.config = config;
            self.config.bind_address = addr;
        } else {
            self.config = config;
        }
        self
    }
    
    /// 启动QUIC服务器
    pub async fn build(self) -> Result<QuicServer, QuicError> {
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        
        // 在实际实现中，这里会创建QUIC Endpoint
        // 目前我们创建一个模拟的服务器
        
        Ok(QuicServer {
            bind_addr,
            config: self.config,
        })
    }
}

impl Default for QuicServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// QUIC服务器
pub struct QuicServer {
    bind_addr: std::net::SocketAddr,
    config: QuicConfig,
}

impl QuicServer {
    /// 创建服务器构建器
    pub fn builder() -> QuicServerBuilder {
        QuicServerBuilder::new()
    }
    
    /// 接受新的QUIC连接
    pub async fn accept(&mut self) -> Result<QuicAdapter, QuicError> {
        // 在实际实现中，这里会等待QUIC连接
        // 目前我们模拟接受一个连接
        
        // 模拟连接等待
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        let peer_addr = "127.0.0.1:12345".parse().unwrap(); // 模拟的对端地址
        Ok(QuicAdapter::new(self.config.clone(), self.bind_addr, peer_addr))
    }
    
    /// 获取本地地址
    pub fn local_addr(&self) -> std::net::SocketAddr {
        self.bind_addr
    }
}

/// QUIC客户端构建器
pub struct QuicClientBuilder {
    config: QuicConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl QuicClientBuilder {
    /// 创建新的QUIC客户端构建器
    pub fn new() -> Self {
        Self {
            config: QuicConfig::default(),
            target_address: None,
        }
    }
    
    /// 设置目标地址
    pub fn target_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_address = Some(addr);
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: QuicConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 连接到QUIC服务器
    pub async fn connect(self) -> Result<QuicAdapter, QuicError> {
        let target_addr = self.target_address
            .ok_or_else(|| QuicError::Connection("Target address not set".to_string()))?;
        
        QuicAdapter::connect(target_addr, self.config).await
    }
}

impl Default for QuicClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// 为了完整性，添加一些模拟的数据注入方法（仅用于测试）
impl QuicAdapter {
    /// 模拟接收数据（仅用于测试）
    pub fn simulate_receive(&mut self, packet: UnifiedPacket) {
        self.receive_buffer.push(packet);
    }
    
    /// 获取发送缓冲区的内容（仅用于测试）
    pub fn get_sent_packets(&self) -> &Vec<UnifiedPacket> {
        &self.send_buffer
    }
} 