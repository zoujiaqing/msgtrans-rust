use async_trait::async_trait;
use tokio::{
    net::{TcpStream, TcpListener},
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{mpsc, broadcast},
};
use std::io;
use crate::{
    SessionId,
    error::TransportError,
    packet::{Packet, PacketError},
    command::{ConnectionInfo, ConnectionState},
    protocol::{ProtocolAdapter, AdapterStats, TcpClientConfig, TcpServerConfig},
    event::TransportEvent,
};
use std::sync::Arc;

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

/// TCP协议适配器 - 事件驱动版本
pub struct TcpAdapter<C> {
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
}

impl<C> TcpAdapter<C> {
    /// 创建新的TCP适配器
    pub async fn new(stream: TcpStream, config: C, event_sender: broadcast::Sender<TransportEvent>) -> Result<Self, TcpError> {
        // 设置基本TCP选项
        stream.set_nodelay(true)?;
        
        let local_addr = stream.local_addr()?;
        let peer_addr = stream.peer_addr()?;
        
        let mut connection_info = ConnectionInfo::default();
        connection_info.local_addr = local_addr;
        connection_info.peer_addr = peer_addr;
        connection_info.protocol = "tcp".to_string();
        connection_info.state = ConnectionState::Connected;
        connection_info.established_at = std::time::SystemTime::now();
        
        let session_id = Arc::new(std::sync::atomic::AtomicU64::new(0)); // 临时ID，稍后会被设置
        
        // 创建通信通道
        let (send_queue_tx, send_queue_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        
        // 启动事件循环
        let event_loop_handle = Self::start_event_loop(
            stream,
            session_id.clone(),
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
        })
    }
    
    /// 获取事件流接收器
    /// 
    /// 这允许客户端订阅TCP适配器内部事件循环发送的事件
    pub fn subscribe_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_sender.subscribe()
    }

    /// 启动基于 tokio::select! 的事件循环
    async fn start_event_loop(
        stream: TcpStream,
        session_id: Arc<std::sync::atomic::AtomicU64>,
        mut send_queue: mpsc::UnboundedReceiver<Packet>,
        mut shutdown_signal: mpsc::UnboundedReceiver<()>,
        event_sender: broadcast::Sender<TransportEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
            tracing::debug!("🚀 TCP事件循环启动 (会话: {})", current_session_id);
            
            // 分离读写流
            let (mut read_half, mut write_half) = stream.into_split();
            
            loop {
                // 获取当前会话ID
                let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
                
                tokio::select! {
                    // 🔍 处理接收数据
                    read_result = Self::read_packet_from_stream(&mut read_half) => {
                        match read_result {
                            Ok(Some(packet)) => {
                                tracing::debug!("📥 TCP接收到数据包: {} bytes (会话: {})", packet.payload.len(), current_session_id);
                                
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
                                tracing::info!("🔗 TCP连接已关闭 (会话: {})", current_session_id);
                                break;
                            }
                            Err(e) => {
                                tracing::error!("📥 TCP读取错误: {:?} (会话: {})", e, current_session_id);
                                break;
                            }
                        }
                    }
                    
                    // 📤 处理发送数据
                    packet = send_queue.recv() => {
                        if let Some(packet) = packet {
                            match Self::write_packet_to_stream(&mut write_half, &packet).await {
                                Ok(_) => {
                                    tracing::debug!("📤 TCP发送成功: {} bytes (会话: {})", packet.payload.len(), current_session_id);
                                    
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
                                    tracing::error!("📤 TCP发送错误: {:?} (会话: {})", e, current_session_id);
                                    break;
                                }
                            }
                        }
                    }
                    
                    // 🛑 处理关闭信号
                    _ = shutdown_signal.recv() => {
                        tracing::info!("🛑 收到关闭信号，停止TCP事件循环 (会话: {})", current_session_id);
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
            }
            
            tracing::debug!("✅ TCP事件循环已结束 (会话: {})", final_session_id);
        })
    }
    
    /// 从流中读取数据包
    async fn read_packet_from_stream(read_half: &mut tokio::net::tcp::OwnedReadHalf) -> Result<Option<Packet>, TcpError> {
        // 读取包头（9字节）
        let mut header_buf = [0u8; 9];
        match read_half.read_exact(&mut header_buf).await {
            Ok(_) => {}
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::ConnectionReset | 
                    std::io::ErrorKind::ConnectionAborted |
                    std::io::ErrorKind::BrokenPipe |
                    std::io::ErrorKind::UnexpectedEof => {
                        return Ok(None); // 连接正常关闭
                    }
                    _ => {
                        return Err(TcpError::Io(e));
                    }
                }
            }
        }
        
        // 解析包头获取负载长度
        let payload_len = u32::from_be_bytes([header_buf[5], header_buf[6], header_buf[7], header_buf[8]]) as usize;
        
        // 防止恶意的大数据包
        if payload_len > 1024 * 1024 { // 1MB 限制
            return Err(TcpError::BufferOverflow);
        }
        
        // 读取负载
        let mut payload = vec![0u8; payload_len];
        read_half.read_exact(&mut payload).await.map_err(TcpError::Io)?;
        
        // 重构完整的数据包
        let mut packet_data = Vec::with_capacity(9 + payload_len);
        packet_data.extend_from_slice(&header_buf);
        packet_data.extend_from_slice(&payload);
        
        // 解析数据包
        let packet = Packet::from_bytes(&packet_data).map_err(TcpError::Packet)?;
        Ok(Some(packet))
    }
    
    /// 向流中写入数据包
    async fn write_packet_to_stream(write_half: &mut tokio::net::tcp::OwnedWriteHalf, packet: &Packet) -> Result<(), TcpError> {
        let packet_bytes = packet.to_bytes();
        write_half.write_all(&packet_bytes).await.map_err(TcpError::Io)?;
        write_half.flush().await.map_err(TcpError::Io)?;
        Ok(())
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
        
        Self::new(stream, config, broadcast::channel(16).0).await
    }
}

#[async_trait]
impl ProtocolAdapter for TcpAdapter<TcpClientConfig> {
    type Config = TcpClientConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        let current_session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        tracing::debug!("📤 TCP发送数据包: {} bytes (会话: {})", packet.payload.len(), current_session_id);
        
        // 通过队列发送数据包，事件循环会处理实际的发送
        self.send_queue.send(packet)
            .map_err(|_| TcpError::ConnectionClosed)?;
        
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        let current_session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        tracing::debug!("🔗 关闭TCP连接 (会话: {})", current_session_id);
        
        // 发送关闭信号
        let _ = self.shutdown_sender.send(());
        
        // 等待事件循环结束
        if let Some(handle) = self.event_loop_handle.take() {
            let _ = handle.await;
        }
        
        self.connection_info.state = ConnectionState::Closed;
        self.connection_info.closed_at = Some(std::time::SystemTime::now());
        
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        self.connection_info.state == ConnectionState::Connected
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst))
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id.store(session_id.0, std::sync::atomic::Ordering::SeqCst);
        self.connection_info.session_id = session_id;
    }
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 在事件驱动模式下，这个方法不再需要
        // 数据可读性由事件循环处理
        Ok(true)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 在事件驱动模式下，flush由事件循环自动处理
        Ok(())
    }
}

// 服务端适配器实现
#[async_trait]
impl ProtocolAdapter for TcpAdapter<TcpServerConfig> {
    type Config = TcpServerConfig;
    type Error = TcpError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        let current_session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        tracing::debug!("📤 TCP发送数据包: {} bytes (会话: {})", packet.payload.len(), current_session_id);
        
        // 通过队列发送数据包，事件循环会处理实际的发送
        self.send_queue.send(packet)
            .map_err(|_| TcpError::ConnectionClosed)?;
        
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        let current_session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        tracing::debug!("🔗 关闭TCP连接 (会话: {})", current_session_id);
        
        // 发送关闭信号
        let _ = self.shutdown_sender.send(());
        
        // 等待事件循环结束
        if let Some(handle) = self.event_loop_handle.take() {
            let _ = handle.await;
        }
        
        self.connection_info.state = ConnectionState::Closed;
        self.connection_info.closed_at = Some(std::time::SystemTime::now());
        
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection_info.clone()
    }
    
    fn is_connected(&self) -> bool {
        self.connection_info.state == ConnectionState::Connected
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst))
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id.store(session_id.0, std::sync::atomic::Ordering::SeqCst);
        self.connection_info.session_id = session_id;
    }
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 在事件驱动模式下，这个方法不再需要
        // 数据可读性由事件循环处理
        Ok(true)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 在事件驱动模式下，flush由事件循环自动处理
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
        
        TcpAdapter::new(stream, self.config.clone(), broadcast::channel(16).0).await
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