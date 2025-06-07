use async_trait::async_trait;
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};
use futures::{SinkExt, StreamExt};
use std::io;
use bytes::BytesMut;
use crate::{
    SessionId, 
    protocol::{ProtocolAdapter, AdapterStats, WebSocketConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
    packet::{Packet, PacketType},
};

/// WebSocket适配器错误类型
#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Invalid message type")]
    InvalidMessageType,
    
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<WebSocketError> for TransportError {
    fn from(error: WebSocketError) -> Self {
        match error {
            WebSocketError::WebSocket(ws_err) => TransportError::Protocol(format!("WebSocket error: {}", ws_err)),
            WebSocketError::Io(io_err) => TransportError::Io(io_err),
            WebSocketError::ConnectionClosed => TransportError::Connection("Connection closed".to_string()),
            WebSocketError::InvalidMessageType => TransportError::Protocol("Invalid message type".to_string()),
            WebSocketError::Serialization(msg) => TransportError::Serialization(msg),
        }
    }
}

/// WebSocket协议适配器
/// 
/// 实现了WebSocket连接的发送和接收功能
pub struct WebSocketAdapter<S> {
    /// WebSocket流
    stream: WebSocketStream<S>,
    /// 会话ID
    session_id: SessionId,
    /// 配置
    #[allow(dead_code)]
    config: WebSocketConfig,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// 连接状态
    is_connected: bool,
}

impl<S> WebSocketAdapter<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    /// 创建新的WebSocket适配器
    pub fn new(
        stream: WebSocketStream<S>,
        config: WebSocketConfig,
        local_addr: std::net::SocketAddr,
        peer_addr: std::net::SocketAddr,
    ) -> Self {
        let mut connection_info = ConnectionInfo::default();
        connection_info.local_addr = local_addr;
        connection_info.peer_addr = peer_addr;
        connection_info.protocol = ProtocolType::WebSocket;
        connection_info.state = ConnectionState::Connected;
        connection_info.established_at = std::time::SystemTime::now();
        
        Self {
            stream,
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection_info,
            is_connected: true,
        }
    }
    
    /// 序列化数据包为WebSocket消息
    fn serialize_packet(&self, packet: &Packet) -> Result<Message, WebSocketError> {
        // 使用统一数据包的标准格式：[类型:1字节][消息ID:4字节][负载长度:4字节][负载]
        let serialized = packet.to_bytes();
        Ok(Message::Binary(serialized.to_vec()))
    }
    
    /// 反序列化WebSocket消息为数据包
    fn deserialize_message(&self, message: Message) -> Result<Option<Packet>, WebSocketError> {
        match message {
            Message::Binary(data) => {
                if data.len() < 9 {
                    return Err(WebSocketError::Serialization("数据太短".to_string()));
                }
                
                // 使用统一数据包的反序列化方法
                match Packet::from_bytes(&data) {
                    Ok(packet) => Ok(Some(packet)),
                    Err(e) => Err(WebSocketError::Serialization(format!("数据包解析失败: {}", e))),
                }
            }
            Message::Text(text) => {
                // 将文本消息转换为二进制数据包（类型为0）
                Ok(Some(Packet {
                    packet_type: PacketType::Data,
                    message_id: 0,
                    payload: BytesMut::from(text.as_bytes()),
                }))
            }
            Message::Ping(_data) => {
                // 自动回应Pong
                // 这里我们不返回数据包，而是在内部处理
                Ok(None)
            }
            Message::Pong(_) => {
                // Pong响应，忽略
                Ok(None)
            }
            Message::Close(_) => {
                // 连接关闭，不需要异步处理
                Ok(None)
            }
            Message::Frame(_) => {
                // 原始帧，通常不需要处理
                Ok(None)
            }
        }
    }
    
    /// 处理连接关闭
    #[allow(dead_code)]
    async fn handle_close(&self) {
        // 这是一个异步方法，但在当前上下文中我们不能修改self
        // 实际的关闭处理应该在调用者中进行
    }
}

#[async_trait]
impl<S> ProtocolAdapter for WebSocketAdapter<S>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    type Config = WebSocketConfig;
    type Error = WebSocketError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(WebSocketError::ConnectionClosed);
        }
        
        let message = self.serialize_packet(&packet)?;
        let message_size = match &message {
            Message::Binary(data) => data.len(),
            Message::Text(text) => text.len(),
            _ => 0,
        };
        
        self.stream.send(message).await?;
        
        // 记录统计信息
        self.stats.record_packet_sent(message_size);
        self.connection_info.record_packet_sent(message_size);
        
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }
        
        loop {
            match self.stream.next().await {
                Some(Ok(message)) => {
                    let message_size = match &message {
                        Message::Binary(data) => data.len(),
                        Message::Text(text) => text.len(),
                        _ => 0,
                    };
                    
                    // 处理Ping消息
                    if let Message::Ping(data) = &message {
                        // 自动发送Pong响应
                        if let Err(e) = self.stream.send(Message::Pong(data.clone())).await {
                            tracing::warn!("Failed to send pong: {}", e);
                        }
                        continue; // 继续等待下一个消息
                    }
                    
                    // 处理Close消息
                    if matches!(message, Message::Close(_)) {
                        self.is_connected = false;
                        self.connection_info.state = ConnectionState::Closed;
                        return Ok(None);
                    }
                    
                    match self.deserialize_message(message)? {
                        Some(packet) => {
                            // 记录统计信息
                            self.stats.record_packet_received(message_size);
                            self.connection_info.record_packet_received(message_size);
                            return Ok(Some(packet));
                        }
                        None => {
                            // 继续等待下一个消息（例如处理了Ping/Pong）
                            continue;
                        }
                    }
                }
                Some(Err(e)) => {
                    self.stats.record_error();
                    self.is_connected = false;
                    return Err(WebSocketError::WebSocket(e));
                }
                None => {
                    // 流结束
                    self.is_connected = false;
                    self.connection_info.state = ConnectionState::Closed;
                    return Ok(None);
                }
            }
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            let _ = self.stream.send(Message::Close(None)).await;
            self.stream.close(None).await?;
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
        // WebSocket流通过future就绪状态检查
        // 这里我们简单返回连接状态
        Ok(self.is_connected)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.stream.flush().await?;
        Ok(())
    }
}

/// WebSocket服务器构建器
pub struct WebSocketServerBuilder {
    config: WebSocketConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl WebSocketServerBuilder {
    /// 创建新的WebSocket服务器构建器
    pub fn new() -> Self {
        Self {
            config: WebSocketConfig::default(),
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
    pub fn config(mut self, config: WebSocketConfig) -> Self {
        if let Some(addr) = self.bind_address {
            self.config = config;
            self.config.bind_address = addr;
        } else {
            self.config = config;
        }
        self
    }
    
    /// 启动WebSocket服务器
    pub async fn build(self) -> Result<WebSocketServer, WebSocketError> {
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        let listener = tokio::net::TcpListener::bind(bind_addr).await?;
        
        Ok(WebSocketServer {
            listener,
            config: self.config,
        })
    }
}

impl Default for WebSocketServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// WebSocket服务器
pub struct WebSocketServer {
    listener: tokio::net::TcpListener,
    config: WebSocketConfig,
}

impl WebSocketServer {
    /// 创建服务器构建器
    pub fn builder() -> WebSocketServerBuilder {
        WebSocketServerBuilder::new()
    }
    
    /// 接受新的WebSocket连接
    pub async fn accept(&mut self) -> Result<WebSocketAdapter<tokio::net::TcpStream>, WebSocketError> {
        let (stream, peer_addr) = self.listener.accept().await?;
        let local_addr = stream.local_addr()?;
        
        // 执行WebSocket握手
        let ws_stream = tokio_tungstenite::accept_async(stream).await?;
        
        Ok(WebSocketAdapter::new(ws_stream, self.config.clone(), local_addr, peer_addr))
    }
    
    /// 获取本地地址
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, WebSocketError> {
        self.listener.local_addr().map_err(WebSocketError::Io)
    }
}

/// WebSocket客户端构建器
pub struct WebSocketClientBuilder {
    config: WebSocketConfig,
    target_url: Option<String>,
}

impl WebSocketClientBuilder {
    /// 创建新的WebSocket客户端构建器
    pub fn new() -> Self {
        Self {
            config: WebSocketConfig::default(),
            target_url: None,
        }
    }
    
    /// 设置目标URL
    pub fn target_url<S: Into<String>>(mut self, url: S) -> Self {
        self.target_url = Some(url.into());
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: WebSocketConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 连接到WebSocket服务器
    pub async fn connect(self) -> Result<WebSocketAdapter<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, WebSocketError> {
        let url = self.target_url
            .ok_or_else(|| WebSocketError::Io(io::Error::new(io::ErrorKind::InvalidInput, "Target URL not set")))?;
        
        let parsed_url = url::Url::parse(&url)
            .map_err(|e| WebSocketError::Serialization(format!("Invalid URL: {}", e)))?;
        
        let (ws_stream, _response) = tokio_tungstenite::connect_async(&url).await?;
        
        // 提取地址信息
        let host = parsed_url.host_str().unwrap_or("unknown");
        let port = parsed_url.port().unwrap_or(if parsed_url.scheme() == "wss" { 443 } else { 80 });
        let peer_addr = format!("{}:{}", host, port).parse()
            .unwrap_or_else(|_| "0.0.0.0:0".parse().unwrap());
        let local_addr = "0.0.0.0:0".parse().unwrap(); // 客户端本地地址通常不重要
        
        Ok(WebSocketAdapter::new(ws_stream, self.config, local_addr, peer_addr))
    }
}

impl Default for WebSocketClientBuilder {
    fn default() -> Self {
        Self::new()
    }
} 