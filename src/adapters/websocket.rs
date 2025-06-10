use async_trait::async_trait;
use tokio_tungstenite::{
    tungstenite::{protocol::Message, Error as TungsteniteError},
    WebSocketStream,
    accept_async, connect_async,
};
use tokio::net::{TcpListener, TcpStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    SessionId, 
    protocol::{ProtocolAdapter, AdapterStats, ProtocolConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
    packet::{Packet, PacketType},
};

#[derive(Debug, thiserror::Error)]
pub enum WebSocketError {
    #[error("Tungstenite error: {0}")]
    Tungstenite(#[from] TungsteniteError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Invalid message type")]
    InvalidMessageType,
    
    #[error("Configuration error: {0}")]
    Config(String),
}

impl From<WebSocketError> for TransportError {
    fn from(error: WebSocketError) -> Self {
        match error {
            WebSocketError::Tungstenite(e) => TransportError::connection_error(format!("WebSocket protocol error: {}", e), true),
            WebSocketError::Io(e) => TransportError::connection_error(format!("WebSocket IO error: {}", e), true),
            WebSocketError::ConnectionClosed => TransportError::connection_error("WebSocket connection closed", false),
            WebSocketError::InvalidMessageType => TransportError::protocol_error("websocket", "Invalid message type"),
            WebSocketError::Config(msg) => TransportError::config_error("websocket", msg),
        }
    }
}

pub struct WebSocketAdapter<C> {
    config: C,
    session_id: SessionId,
    stats: AdapterStats,
    connection_info: ConnectionInfo,
    // WebSocket连接流
    stream: Option<Arc<Mutex<WebSocketStream<TcpStream>>>>,
    is_connected: bool,
}

impl<C> WebSocketAdapter<C> {
    pub fn new(config: C) -> Self {
        Self {
            config,
            session_id: SessionId::new(0),
            stats: AdapterStats::new(),
            connection_info: ConnectionInfo::default(),
            stream: None,
            is_connected: false,
        }
    }
    
    pub fn new_with_stream(config: C, stream: WebSocketStream<TcpStream>) -> Self {
        Self {
            config,
            session_id: SessionId::new(0),
            stats: AdapterStats::new(),
            connection_info: ConnectionInfo::default(),
            stream: Some(Arc::new(Mutex::new(stream))),
            is_connected: true,
        }
    }
}

#[async_trait]
impl<C> ProtocolAdapter for WebSocketAdapter<C>
where
    C: Send + Sync + 'static + ProtocolConfig,
{
    type Config = C;
    type Error = WebSocketError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if let Some(stream) = &self.stream {
            let mut ws_stream = stream.lock().await;
            let serialized_data = packet.to_bytes();
            let message = Message::Binary(serialized_data.to_vec());
            ws_stream.send(message).await?;
            
            // 更新统计信息
            self.stats.record_packet_sent(packet.payload.len());
            Ok(())
        } else {
            Err(WebSocketError::ConnectionClosed)
        }
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if let Some(stream) = &self.stream {
            let mut ws_stream = stream.lock().await;
            
            match ws_stream.next().await {
                Some(Ok(message)) => {
                    match message {
                        Message::Binary(data) => {
                            // 尝试从二进制数据解析Packet
                            match Packet::from_bytes(&data) {
                                Ok(packet) => {
                                    // 更新统计信息
                                    self.stats.record_packet_received(data.len());
                                    Ok(Some(packet))
                                }
                                Err(_) => {
                                    // 如果解析失败，创建一个基本的数据包
                                    let packet = Packet::data(0, &data[..]);
                                    self.stats.record_packet_received(data.len());
                                    Ok(Some(packet))
                                }
                            }
                        }
                        Message::Text(text) => {
                            // 文本消息直接创建数据包
                            let packet = Packet::data(0, text.as_bytes());
                            
                            // 更新统计信息
                            self.stats.record_packet_received(text.len());
                            Ok(Some(packet))
                        }
                        Message::Close(_) => {
                            self.is_connected = false;
                            Err(WebSocketError::ConnectionClosed)
                        }
                        Message::Ping(_) | Message::Pong(_) => {
                            // 心跳消息，继续接收下一个消息
                            Ok(None)
                        }
                        Message::Frame(_) => {
                            Err(WebSocketError::InvalidMessageType)
                        }
                    }
                }
                Some(Err(e)) => {
                    self.is_connected = false;
                    Err(WebSocketError::Tungstenite(e))
                }
                None => {
                    self.is_connected = false;
                    Err(WebSocketError::ConnectionClosed)
                }
            }
        } else {
            Err(WebSocketError::ConnectionClosed)
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if let Some(stream) = &self.stream {
            let mut ws_stream = stream.lock().await;
            ws_stream.close(None).await?;
            self.is_connected = false;
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
    }
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        Ok(false)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub(crate) struct WebSocketServerBuilder<C> {
    config: Option<C>,
}

impl<C> WebSocketServerBuilder<C> {
    pub(crate) fn new() -> Self {
        Self { config: None }
    }
    
    pub(crate) fn config(mut self, config: C) -> Self {
        self.config = Some(config);
        self
    }
    
    pub(crate) fn bind_address(self, _addr: std::net::SocketAddr) -> Self {
        self
    }
    
    pub(crate) async fn build(self) -> Result<WebSocketServer<C>, WebSocketError> {
        let config = self.config.ok_or_else(|| WebSocketError::Config("Missing WebSocket server config".to_string()))?;
        Ok(WebSocketServer { config })
    }
}

pub(crate) struct WebSocketServer<C> {
    config: C,
}

impl<C> WebSocketServer<C> {
    pub(crate) async fn accept(&mut self) -> Result<WebSocketAdapter<C>, WebSocketError>
    where
        C: Clone,
    {
        // TODO: 实现WebSocket服务器accept逻辑
        Err(WebSocketError::Config("WebSocket server accept not implemented yet".to_string()))
    }
    
    pub(crate) fn local_addr(&self) -> Result<std::net::SocketAddr, WebSocketError> {
        // TODO: 实现获取本地地址的逻辑
        Err(WebSocketError::Config("WebSocket server local_addr not implemented yet".to_string()))
    }
}

pub(crate) struct WebSocketClientBuilder<C> {
    config: Option<C>,
}

impl<C> WebSocketClientBuilder<C> {
    pub(crate) fn new() -> Self {
        Self { config: None }
    }
    
    pub(crate) fn config(mut self, config: C) -> Self {
        self.config = Some(config);
        self
    }
    
    pub(crate) fn target_url<S: Into<String>>(self, _url: S) -> Self {
        self
    }
    
    pub(crate) async fn connect(self) -> Result<WebSocketAdapter<C>, WebSocketError> {
        let config = self.config.ok_or_else(|| WebSocketError::Config("Missing WebSocket client config".to_string()))?;
        // TODO: 实现实际的WebSocket客户端连接逻辑
        Ok(WebSocketAdapter::new(config))
    }
} 