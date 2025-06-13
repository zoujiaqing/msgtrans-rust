use async_trait::async_trait;
use tokio_tungstenite::{MaybeTlsStream,
    tungstenite::{protocol::Message, Error as TungsteniteError, error},
    WebSocketStream,
    accept_async, connect_async,
};
use tokio::net::{TcpListener, TcpStream};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::{
    SessionId,
    error::TransportError,
    packet::Packet,
    protocol::{ProtocolAdapter, AdapterStats, ProtocolConfig},
    command::{ConnectionInfo, ConnectionState},
    event::TransportEvent,
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

/// WebSocket协议适配器 - 事件驱动版本
pub struct WebSocketAdapter<C> {
    /// 会话ID (使用原子类型以便事件循环访问)
    session_id: Arc<std::sync::atomic::AtomicU64>,
    /// 配置
    config: C,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// 发送队列
    send_queue: mpsc::UnboundedSender<Packet>,
    /// 事件发送器
    event_sender: broadcast::Sender<TransportEvent>,
    /// 关闭信号发送器
    shutdown_sender: mpsc::UnboundedSender<()>,
    /// 事件循环句柄
    event_loop_handle: Option<tokio::task::JoinHandle<()>>,
    /// 连接状态
    is_connected: Arc<std::sync::atomic::AtomicBool>,
}

impl<C> WebSocketAdapter<C> {
    pub fn new(config: C) -> Self {
        let (event_sender, _) = broadcast::channel(1000);
        let (send_queue_tx, _) = mpsc::unbounded_channel();
        let (shutdown_tx, _) = mpsc::unbounded_channel();
        
        Self {
            session_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            config,
            stats: AdapterStats::new(),
            connection_info: ConnectionInfo::default(),
            send_queue: send_queue_tx,
            event_sender,
            shutdown_sender: shutdown_tx,
            event_loop_handle: None,
            is_connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
    
    /// 创建带有WebSocket流的适配器
    pub async fn new_with_stream(config: C, stream: WebSocketStream<MaybeTlsStream<TcpStream>>, event_sender: broadcast::Sender<TransportEvent>) -> Result<Self, WebSocketError> {
        let mut connection_info = ConnectionInfo::default();
        connection_info.protocol = "websocket".to_string();
        connection_info.state = ConnectionState::Connected;
        connection_info.established_at = std::time::SystemTime::now();
        
        let session_id = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let is_connected = Arc::new(std::sync::atomic::AtomicBool::new(true));
        
        // 创建通信通道
        let (send_queue_tx, send_queue_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        
        // 启动事件循环
        let event_loop_handle = Self::start_event_loop(
            stream,
            session_id.clone(),
            is_connected.clone(),
            send_queue_rx,
            shutdown_rx,
            event_sender.clone(),
        ).await;
        
        Ok(Self {
            session_id,
            config,
            stats: AdapterStats::new(),
            connection_info,
            send_queue: send_queue_tx,
            event_sender,
            shutdown_sender: shutdown_tx,
            event_loop_handle: Some(event_loop_handle),
            is_connected,
        })
    }
    
    /// 获取事件流接收器
    pub fn subscribe_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_sender.subscribe()
    }

    /// 启动基于 tokio::select! 的事件循环
    async fn start_event_loop(
        mut stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
        session_id: Arc<std::sync::atomic::AtomicU64>,
        is_connected: Arc<std::sync::atomic::AtomicBool>,
        mut send_queue: mpsc::UnboundedReceiver<Packet>,
        mut shutdown_signal: mpsc::UnboundedReceiver<()>,
        event_sender: broadcast::Sender<TransportEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
            tracing::debug!("🚀 WebSocket事件循环启动 (会话: {})", current_session_id);
            
            loop {
                // 获取当前会话ID
                let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
                
                tokio::select! {
                    // 🔍 处理接收数据
                    read_result = stream.next() => {
                        match read_result {
                            Some(Ok(message)) => {
                                match Self::process_websocket_message(message) {
                                    Ok(Some(packet)) => {
                                        tracing::debug!("📥 WebSocket接收到数据包: {} bytes (会话: {})", packet.payload.len(), current_session_id);
                                        
                                        // 发送接收事件
                                        let event = TransportEvent::MessageReceived {
                                            session_id: current_session_id,
                                            packet,
                                        };
                                        
                                        if let Err(e) = event_sender.send(event) {
                                            tracing::warn!("📥 发送接收事件失败: {:?}", e);
                                        }
                                    }
                                    Ok(None) => {
                                        // 心跳消息，继续循环
                                        continue;
                                    }
                                    Err(e) => {
                                        tracing::error!("📥 WebSocket消息处理错误: {:?} (会话: {})", e, current_session_id);
                                        is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                                        break;
                                    }
                                }
                            }
                            Some(Err(e)) => {
                                // 优雅处理不同类型的WebSocket错误
                                match e {
                                    TungsteniteError::Protocol(error::ProtocolError::ResetWithoutClosingHandshake) => {
                                        tracing::info!("🔗 WebSocket连接被客户端重置 (会话: {})", current_session_id);
                                    }
                                    TungsteniteError::ConnectionClosed => {
                                        tracing::info!("🔗 WebSocket连接正常关闭 (会话: {})", current_session_id);
                                    }
                                    _ => {
                                        tracing::warn!("📥 WebSocket读取错误: {:?} (会话: {})", e, current_session_id);
                                    }
                                }
                                is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                                break;
                            }
                            None => {
                                tracing::info!("🔗 WebSocket连接已关闭 (会话: {})", current_session_id);
                                is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                                break;
                            }
                        }
                    }
                    
                    // 📤 处理发送数据
                    packet = send_queue.recv() => {
                        if let Some(packet) = packet {
                            let serialized_data = packet.to_bytes();
                            let message = Message::Binary(serialized_data.to_vec());
                            
                            match stream.send(message).await {
                                Ok(_) => {
                                    tracing::debug!("📤 WebSocket发送成功: {} bytes (会话: {})", packet.payload.len(), current_session_id);
                                    
                                    // 发送发送事件
                                    let event = TransportEvent::MessageSent {
                                        session_id: current_session_id,
                                        packet_id: packet.message_id,
                                    };
                                    
                                    if let Err(e) = event_sender.send(event) {
                                        tracing::warn!("📤 发送发送事件失败: {:?}", e);
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("📤 WebSocket发送错误: {:?} (会话: {})", e, current_session_id);
                                    is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
                                    break;
                                }
                            }
                        }
                    }
                    
                    // 🛑 处理关闭信号
                    _ = shutdown_signal.recv() => {
                        tracing::info!("🛑 收到关闭信号，停止WebSocket事件循环 (会话: {})", current_session_id);
                        break;
                    }
                }
            }
            
            // 发送连接关闭事件
            let final_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
            let close_event = TransportEvent::ConnectionClosed {
                session_id: final_session_id,
                reason: crate::error::CloseReason::Normal,
            };
            
            if let Err(e) = event_sender.send(close_event) {
                tracing::debug!("🔗 连接关闭事件未发送（接收器已关闭，正常情况）: 会话 {}", final_session_id);
            } else {
                tracing::debug!("✅ 关闭事件发送成功 (会话: {})", final_session_id);
            }
            
            tracing::debug!("✅ WebSocket事件循环已结束 (会话: {})", final_session_id);
        })
    }
    
    /// 处理WebSocket消息
    fn process_websocket_message(message: Message) -> Result<Option<Packet>, WebSocketError> {
        match message {
            Message::Binary(data) => {
                // 尝试从二进制数据解析Packet
                match Packet::from_bytes(&data) {
                    Ok(packet) => Ok(Some(packet)),
                    Err(_) => {
                        // 如果解析失败，创建一个基本的数据包
                        let packet = Packet::data(0, &data[..]);
                        Ok(Some(packet))
                    }
                }
            }
            Message::Text(text) => {
                // 文本消息直接创建数据包
                let packet = Packet::data(0, text.as_bytes());
                Ok(Some(packet))
            }
            Message::Close(_) => {
                Err(WebSocketError::ConnectionClosed)
            }
            Message::Ping(_) | Message::Pong(_) => {
                // 心跳消息，返回None表示继续处理
                Ok(None)
            }
            Message::Frame(_) => {
                Err(WebSocketError::InvalidMessageType)
            }
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
        // 使用发送队列而不是直接发送
        self.send_queue.send(packet).map_err(|_| WebSocketError::ConnectionClosed)?;
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        // 发送关闭信号
        let _ = self.shutdown_sender.send(());
        
        // 等待事件循环结束
        if let Some(handle) = self.event_loop_handle.take() {
            let _ = handle.await;
        }
        
        self.is_connected.store(false, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected.load(std::sync::atomic::Ordering::SeqCst)
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst))
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id.store(session_id.0, std::sync::atomic::Ordering::SeqCst);
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
                ws_config.bind_address.to_string()
            } else {
                "127.0.0.1:8080".parse().unwrap()
            };
            
            let listener = TcpListener::bind(&bind_addr).await?;
            tracing::debug!("🚀 WebSocket服务器启动在: {}", bind_addr);
            self.listener = Some(listener);
        }
        
        if let Some(listener) = &self.listener {
            let (tcp_stream, addr) = listener.accept().await?;
            tracing::debug!("✅ WebSocket服务器接受连接: {}", addr);
            
            // 执行WebSocket握手
            let maybe_tls_stream = MaybeTlsStream::Plain(tcp_stream); let ws_stream = accept_async(maybe_tls_stream).await?;
            
            // 创建事件发送器
            let (event_sender, _) = broadcast::channel(1000);
            
            // 创建WebSocket适配器
            WebSocketAdapter::new_with_stream(self.config.clone(), ws_stream, event_sender).await
        } else {
            Err(WebSocketError::Config("No listener available".to_string()))
        }
    }
    
    pub(crate) fn local_addr(&self) -> Result<std::net::SocketAddr, WebSocketError> {
        if let Some(listener) = &self.listener {
            listener.local_addr().map_err(WebSocketError::Io)
        } else {
            Err(WebSocketError::Config("Server not bound".to_string()))
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
        
        // 从配置中获取连接URL
        let url = if let Some(ws_config) = (&config as &dyn std::any::Any).downcast_ref::<crate::protocol::WebSocketClientConfig>() {
            ws_config.target_url.clone()
        } else {
            "ws://127.0.0.1:8080".to_string()
        };
        
        tracing::debug!("🔌 WebSocket客户端连接到: {}", url);
        
        // 连接到WebSocket服务器
        let (ws_stream, _) = connect_async(&url).await?;
        
        tracing::debug!("✅ WebSocket客户端已连接到: {}", url);
        
        // 创建事件发送器
        let (event_sender, _) = broadcast::channel(1000);
        
        // 创建WebSocket适配器
        WebSocketAdapter::new_with_stream(config, ws_stream, event_sender).await
    }
} 