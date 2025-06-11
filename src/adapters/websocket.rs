use async_trait::async_trait;
use tokio_tungstenite::{
    tungstenite::{protocol::Message, Error as TungsteniteError},
    WebSocketStream, MaybeTlsStream,
    accept_async, connect_async,
};
use tokio::net::{TcpListener, TcpStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    SessionId,
    error::TransportError,
    packet::Packet,
    protocol::{ProtocolAdapter, AdapterStats, ProtocolConfig, WebSocketClientConfig, WebSocketServerConfig},
    command::{ConnectionInfo, ConnectionState},
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
    // WebSocket连接流 - 支持TLS和非TLS连接
    stream: Option<Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
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
    
    pub fn new_with_stream(config: C, stream: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
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
        Ok(WebSocketServer { config, listener: None })
    }
}

pub(crate) struct WebSocketServer<C> {
    config: C,
    listener: Option<TcpListener>,
}

impl<C: 'static> WebSocketServer<C> {
    pub(crate) async fn accept(&mut self) -> Result<WebSocketAdapter<C>, WebSocketError>
    where
        C: Clone + crate::protocol::ProtocolConfig,
    {
        // 如果还没有监听器，先创建一个
        if self.listener.is_none() {
            let bind_addr = if let Some(ws_config) = (&self.config as &dyn std::any::Any).downcast_ref::<crate::protocol::WebSocketServerConfig>() {
                ws_config.bind_address
            } else {
                return Err(WebSocketError::Config("Invalid WebSocket server config type".to_string()));
            };
            
            let listener = TcpListener::bind(bind_addr).await?;
            tracing::info!("🌐 WebSocket 服务器监听: {}", bind_addr);
            self.listener = Some(listener);
        }
        
        // 接受新的TCP连接
        let listener = self.listener.as_ref().unwrap();
        let (tcp_stream, peer_addr) = listener.accept().await?;
        
        // 将TcpStream包装为MaybeTlsStream（非TLS）
        let maybe_tls_stream = MaybeTlsStream::Plain(tcp_stream);
        
        // 执行WebSocket握手
        let ws_stream = accept_async(maybe_tls_stream).await?;
        tracing::info!("✅ WebSocket 连接已建立，来自: {}", peer_addr);
        
        // 创建连接信息
        let local_addr = self.local_addr()?;
        let now = std::time::SystemTime::now();
        let connection_info = crate::command::ConnectionInfo {
            session_id: crate::SessionId::new(0), // 临时ID，稍后会被设置
            local_addr,
            peer_addr,
            protocol: "websocket".to_string(),
            state: crate::command::ConnectionState::Connected,
            established_at: now,
            closed_at: None,
            last_activity: now,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        };
        
        // 创建适配器
        let mut adapter = WebSocketAdapter::new_with_stream(self.config.clone(), ws_stream);
        adapter.connection_info = connection_info;
        adapter.is_connected = true;
        
        Ok(adapter)
    }
    
    pub(crate) fn local_addr(&self) -> Result<std::net::SocketAddr, WebSocketError> {
        // 获取配置中的绑定地址
        if let Some(ws_config) = (&self.config as &dyn std::any::Any).downcast_ref::<crate::protocol::WebSocketServerConfig>() {
            Ok(ws_config.bind_address)
        } else {
            Err(WebSocketError::Config("Invalid WebSocket server config type".to_string()))
        }
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
    
    pub(crate) async fn connect(self) -> Result<WebSocketAdapter<C>, WebSocketError> 
    where
        C: crate::protocol::ProtocolConfig,
    {
        let config = self.config.ok_or_else(|| WebSocketError::Config("Missing WebSocket client config".to_string()))?;
        
        // 获取WebSocket客户端配置
        let ws_config = if let Some(ws_config) = (&config as &dyn std::any::Any).downcast_ref::<crate::protocol::WebSocketClientConfig>() {
            ws_config
        } else {
            return Err(WebSocketError::Config("Invalid WebSocket client config type".to_string()));
        };
        
        // 建立WebSocket连接，使用字符串URL
        let (ws_stream, _response) = connect_async(&ws_config.target_url).await?;
        tracing::info!("🔌 WebSocket客户端已连接到: {}", ws_config.target_url);
        
        // 尝试从URL解析远程地址，使用默认地址作为后备
        let remote_addr = if let Ok(url) = ws_config.target_url.parse::<url::Url>() {
            if let Some(host) = url.host_str() {
                let port = url.port().unwrap_or(if url.scheme() == "wss" { 443 } else { 80 });
                format!("{}:{}", host, port).parse().unwrap_or_else(|_| "127.0.0.1:80".parse().unwrap())
            } else {
                "127.0.0.1:80".parse().unwrap()
            }
        } else {
            "127.0.0.1:80".parse().unwrap()
        };
        
        // 创建连接信息
        let now = std::time::SystemTime::now();
        let connection_info = crate::command::ConnectionInfo {
            session_id: crate::SessionId::new(0), // 临时ID，稍后会被设置
            local_addr: "0.0.0.0:0".parse().unwrap(), // 客户端本地地址通常不确定
            peer_addr: remote_addr,
            protocol: "websocket".to_string(),
            state: crate::command::ConnectionState::Connected,
            established_at: now,
            closed_at: None,
            last_activity: now,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        };
        
        // 创建适配器
        let mut adapter = WebSocketAdapter::new_with_stream(config, ws_stream);
        adapter.connection_info = connection_info;
        adapter.is_connected = true;
        
        Ok(adapter)
    }
} 