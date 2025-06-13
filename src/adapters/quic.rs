/// 🔧 事件驱动QUIC适配器
/// 
/// 这是QUIC适配器的现代化版本，支持：
/// - 双向流复用
/// - 事件驱动架构
/// - 读写分离
/// - 异步队列

use async_trait::async_trait;
use quinn::{
    Endpoint, ServerConfig, ClientConfig, Connection, RecvStream, SendStream,
    ConnectError, ConnectionError, ReadError, WriteError, ClosedStream, ReadToEndError,
    Incoming,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
    RootCertStore,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    DigitallySignedStruct, SignatureScheme,
};
use std::{sync::Arc, net::SocketAddr, time::Duration, convert::TryInto};
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    SessionId, 
    error::TransportError, 
    packet::Packet, 
    command::ConnectionInfo,
    protocol::{ProtocolAdapter, AdapterStats, QuicClientConfig, QuicServerConfig},
    event::TransportEvent,
};

#[derive(Debug, thiserror::Error)]
pub enum QuicError {
    #[error("Quinn connection error: {0}")]
    Connect(#[from] ConnectError),
    
    #[error("Quinn connection error: {0}")]
    Connection(#[from] ConnectionError),
    
    #[error("Quinn read error: {0}")]
    Read(#[from] ReadError),
    
    #[error("Quinn write error: {0}")]
    Write(#[from] WriteError),
    
    #[error("Quinn stream closed: {0}")]
    ClosedStream(#[from] ClosedStream),
    
    #[error("Quinn read to end error: {0}")]
    ReadToEnd(#[from] ReadToEndError),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),
    
    #[error("Connection closed")]
    ConnectionClosed,
    
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
}

impl From<QuicError> for TransportError {
    fn from(error: QuicError) -> Self {
        match error {
            QuicError::Connect(e) => TransportError::connection_error(format!("QUIC connection failed: {}", e), true),
            QuicError::Connection(e) => TransportError::connection_error(format!("QUIC connection error: {}", e), true),
            QuicError::Read(e) => TransportError::connection_error(format!("QUIC read error: {}", e), false),
            QuicError::Write(e) => TransportError::connection_error(format!("QUIC write error: {}", e), false),
            QuicError::ClosedStream(e) => TransportError::connection_error(format!("QUIC stream closed: {}", e), false),
            QuicError::ReadToEnd(e) => TransportError::connection_error(format!("QUIC read to end error: {}", e), false),
            QuicError::Io(e) => TransportError::connection_error(format!("QUIC IO error: {}", e), true),
            QuicError::Tls(e) => TransportError::config_error("quic", format!("TLS error: {}", e)),
            QuicError::ConnectionClosed => TransportError::connection_error("QUIC connection closed", true),
            QuicError::Config(msg) => TransportError::config_error("quic", msg),
            QuicError::Serialization(msg) => TransportError::protocol_error("quic", msg),
        }
    }
}

// Custom verifier that skips server certificate verification
#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::ED25519,
        ]
    }
}

// Certificate generation function
fn generate_self_signed_cert() -> (CertificateDer<'static>, PrivatePkcs8KeyDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    (
        cert.cert.der().clone(),
        PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
    )
}

/// Configure client without certificate verification (insecure for development)
fn configure_client_insecure() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    let mut client_config = ClientConfig::new(
        Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap())
    );
    
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    client_config.transport_config(Arc::new(transport_config));
    
    client_config
}

/// Configure server with self-signed certificate
fn configure_server_insecure() -> (ServerConfig, CertificateDer<'static>) {
    let (cert, key) = generate_self_signed_cert();
    
    let mut server_config = ServerConfig::with_single_cert(
        vec![cert.clone()], 
        key.into()
    ).unwrap();

    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.receive_window((1500u32 * 100).into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));

    (server_config, cert)
}

/// QUIC协议适配器（泛型支持客户端和服务端配置）
pub struct QuicAdapter<C> {
    /// 会话ID (使用原子类型以便事件循环访问)
    session_id: Arc<std::sync::atomic::AtomicU64>,
    config: C,
    stats: AdapterStats,
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

impl<C> QuicAdapter<C> {
    pub async fn new_with_connection(
        connection: Connection, 
        config: C, 
        event_sender: broadcast::Sender<TransportEvent>
    ) -> Result<Self, QuicError> {
        let session_id = Arc::new(std::sync::atomic::AtomicU64::new(0));
        
        // 创建连接信息
        let mut connection_info = ConnectionInfo::default();
        connection_info.protocol = "quic".to_string();
        connection_info.session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
        
        // 获取地址信息
        if let Some(local_addr) = connection.local_ip() {
            connection_info.local_addr = format!("{}:0", local_addr).parse().unwrap_or(connection_info.local_addr);
        }
        
        // 创建通信通道
        let (send_queue_tx, send_queue_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel();
        
        // 启动事件循环
        let event_loop_handle = Self::start_event_loop(
            connection,
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
    /// 这允许客户端订阅QUIC适配器内部事件循环发送的事件
    pub fn subscribe_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_sender.subscribe()
    }

    /// 启动基于 tokio::select! 的事件循环
    async fn start_event_loop(
        connection: Connection,
        session_id: Arc<std::sync::atomic::AtomicU64>,
        mut send_queue: mpsc::UnboundedReceiver<Packet>,
        mut shutdown_signal: mpsc::UnboundedReceiver<()>,
        event_sender: broadcast::Sender<TransportEvent>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
            tracing::debug!("🚀 QUIC事件循环启动 (会话: {})", current_session_id);
            
            loop {
                // 获取当前会话ID
                let current_session_id = SessionId(session_id.load(std::sync::atomic::Ordering::SeqCst));
                
                tokio::select! {
                    // 🔍 处理接收数据
                    recv_result = connection.accept_uni() => {
                        match recv_result {
                            Ok(mut recv_stream) => {
                                match recv_stream.read_to_end(1024 * 1024).await {
                                    Ok(buf) => {
                                        tracing::debug!("📥 QUIC接收到数据包: {} bytes (会话: {})", buf.len(), current_session_id);
                                        
                                        // 尝试解析数据包
                                        let packet = match Packet::from_bytes(&buf) {
                                            Ok(packet) => packet,
                                            Err(_) => {
                                                // 如果解析失败，创建基本数据包
                                                Packet::data(0, &buf[..])
                                            }
                                        };
                                        
                                        // 发送接收事件
                                        let event = TransportEvent::MessageReceived {
                                            session_id: current_session_id,
                                            packet,
                                        };
                                        
                                        if let Err(e) = event_sender.send(event) {
                                            tracing::warn!("📥 发送接收事件失败: {:?}", e);
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("📥 QUIC读取错误: {:?} (会话: {})", e, current_session_id);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("📥 QUIC接收流错误: {:?} (会话: {})", e, current_session_id);
                                break;
                            }
                        }
                    }
                    
                    // 📤 处理发送数据
                    packet = send_queue.recv() => {
                        if let Some(packet) = packet {
                            match connection.open_uni().await {
                                Ok(mut send_stream) => {
                                    let data = packet.to_bytes();
                                    match send_stream.write_all(&data).await {
                                        Ok(_) => {
                                            if let Err(e) = send_stream.finish() {
                                                tracing::error!("📤 QUIC流关闭错误: {:?} (会话: {})", e, current_session_id);
                                            } else {
                                                tracing::debug!("📤 QUIC发送成功: {} bytes (会话: {})", packet.payload.len(), current_session_id);
                                                
                                                // 发送发送事件
                                                let event = TransportEvent::MessageSent {
                                                    session_id: current_session_id,
                                                    packet_id: packet.message_id,
                                                };
                                                
                                                if let Err(e) = event_sender.send(event) {
                                                    tracing::warn!("📤 发送发送事件失败: {:?}", e);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("📤 QUIC发送错误: {:?} (会话: {})", e, current_session_id);
                                            break;
                                        }
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("📤 QUIC打开发送流错误: {:?} (会话: {})", e, current_session_id);
                                    break;
                                }
                            }
                        }
                    }
                    
                    // 🛑 处理关闭信号
                    _ = shutdown_signal.recv() => {
                        tracing::info!("🛑 收到关闭信号，停止QUIC事件循环 (会话: {})", current_session_id);
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
                tracing::warn!("🔗 发送关闭事件失败: {:?}", e);
            }
            
            tracing::debug!("✅ QUIC事件循环已结束 (会话: {})", final_session_id);
        })
    }
}

// 客户端适配器实现
impl QuicAdapter<QuicClientConfig> {
    /// 连接到QUIC服务器
    pub async fn connect(addr: SocketAddr, config: QuicClientConfig) -> Result<Self, QuicError> {
        tracing::debug!("🔌 QUIC客户端连接到: {}", addr);
        
        // 创建客户端端点
        let client_config = configure_client_insecure();
        let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        endpoint.set_default_client_config(client_config);
        
        // 连接到服务器
        let connection = endpoint.connect(addr, "localhost")?.await?;
        tracing::debug!("✅ QUIC客户端已连接到: {}", addr);
        
        Self::new_with_connection(connection, config, broadcast::channel(16).0).await
    }
}

#[async_trait]
impl ProtocolAdapter for QuicAdapter<QuicClientConfig> {
    type Config = QuicClientConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        self.send_queue.send(packet).map_err(|_| QuicError::ConnectionClosed)?;
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // 事件驱动模式下，不直接调用receive，而是通过事件流
        Err(QuicError::Config("Use event stream for receiving messages".to_string()))
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        tracing::debug!("🔌 关闭QUIC客户端连接");
        
        // 发送关闭信号
        if let Err(e) = self.shutdown_sender.send(()) {
            tracing::warn!("发送关闭信号失败: {:?}", e);
        }
        
        // 等待事件循环结束
        if let Some(handle) = self.event_loop_handle.take() {
            if let Err(e) = handle.await {
                tracing::warn!("等待事件循环结束失败: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        let mut info = ConnectionInfo::default();
        info.protocol = "quic".to_string();
        info.session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        info
    }
    
    fn is_connected(&self) -> bool {
        self.event_loop_handle.is_some()
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
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 事件驱动模式下总是可读的
        Ok(true)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // QUIC流会自动刷新
        Ok(())
    }
}

// 服务端适配器实现
#[async_trait]
impl ProtocolAdapter for QuicAdapter<QuicServerConfig> {
    type Config = QuicServerConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        self.send_queue.send(packet).map_err(|_| QuicError::ConnectionClosed)?;
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // 事件驱动模式下，不直接调用receive，而是通过事件流
        Err(QuicError::Config("Use event stream for receiving messages".to_string()))
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        tracing::debug!("🔌 关闭QUIC服务端连接");
        
        // 发送关闭信号
        if let Err(e) = self.shutdown_sender.send(()) {
            tracing::warn!("发送关闭信号失败: {:?}", e);
        }
        
        // 等待事件循环结束
        if let Some(handle) = self.event_loop_handle.take() {
            if let Err(e) = handle.await {
                tracing::warn!("等待事件循环结束失败: {:?}", e);
            }
        }
        
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        let mut info = ConnectionInfo::default();
        info.protocol = "quic".to_string();
        info.session_id = SessionId(self.session_id.load(std::sync::atomic::Ordering::SeqCst));
        
        info
    }
    
    fn is_connected(&self) -> bool {
        self.event_loop_handle.is_some()
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
    
    async fn poll_readable(&mut self) -> Result<bool, Self::Error> {
        // 事件驱动模式下总是可读的
        Ok(true)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // QUIC流会自动刷新
        Ok(())
    }
}

// 服务器构建器和相关结构体保持不变...
pub(crate) struct QuicServerBuilder {
    config: QuicServerConfig,
    bind_address: Option<SocketAddr>,
}

impl QuicServerBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: QuicServerConfig::default(),
            bind_address: None,
        }
    }
    
    pub(crate) fn bind_address(mut self, addr: SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    pub(crate) fn config(mut self, config: QuicServerConfig) -> Self {
        self.config = config;
        self
    }
    
    pub(crate) async fn build(self) -> Result<QuicServer, QuicError> {
        let bind_addr = self.bind_address.unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
        
        let (server_config, _cert) = configure_server_insecure();
        let endpoint = Endpoint::server(server_config, bind_addr)?;
        
        tracing::debug!("🚀 QUIC服务器启动在: {}", endpoint.local_addr()?);
        
        Ok(QuicServer {
            config: self.config,
            endpoint,
        })
    }
}

impl Default for QuicServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct QuicServer {
    config: QuicServerConfig,
    endpoint: Endpoint,
}

impl QuicServer {
    pub(crate) fn builder() -> QuicServerBuilder {
        QuicServerBuilder::new()
    }
    
    pub(crate) async fn accept(&mut self) -> Result<QuicAdapter<QuicServerConfig>, QuicError> {
        let incoming = self.endpoint.accept().await.ok_or(QuicError::ConnectionClosed)?;
        let connection = incoming.await?;
        
        tracing::debug!("✅ QUIC服务器接受连接: {}", connection.remote_address());
        
        QuicAdapter::new_with_connection(connection, self.config.clone(), broadcast::channel(16).0).await
    }
    
    pub(crate) fn local_addr(&self) -> Result<SocketAddr, QuicError> {
        self.endpoint.local_addr().map_err(QuicError::Io)
    }
}

pub(crate) struct QuicClientBuilder {
    config: QuicClientConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl QuicClientBuilder {
    pub(crate) fn new() -> Self {
        Self {
            config: QuicClientConfig::default(),
            target_address: None,
        }
    }
    
    pub(crate) fn target_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_address = Some(addr);
        self
    }
    
    pub(crate) fn config(mut self, config: QuicClientConfig) -> Self {
        self.config = config;
        self
    }
    
    pub(crate) async fn connect(self) -> Result<QuicAdapter<QuicClientConfig>, QuicError> {
        let addr = self.target_address.ok_or_else(|| QuicError::Config("Target address not set".to_string()))?;
        QuicAdapter::connect(addr, self.config).await
    }
}

impl Default for QuicClientBuilder {
    fn default() -> Self {
        Self::new()
    }
} 