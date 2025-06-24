/// 🔧 事件驱动QUIC适配器
/// 
/// 这是QUIC适配器的现代化版本，支持：
/// - 双向流复用
/// - 事件驱动架构
/// - 读写分离
/// - 异步队列

use async_trait::async_trait;
use quinn::{
    Endpoint, ServerConfig, ClientConfig, Connection,
    ConnectError, ConnectionError, ReadError, WriteError, ClosedStream, ReadToEndError,
};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName},
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    DigitallySignedStruct, SignatureScheme,
};
use std::{sync::Arc, net::SocketAddr, time::Duration, convert::TryInto};
use tokio::sync::{broadcast, mpsc};
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

/// Configure client with QuicClientConfig parameters
fn configure_client_with_config(config: &QuicClientConfig) -> Result<ClientConfig, QuicError> {
    let crypto = if config.verify_certificate {
        // 使用证书验证模式
        let mut root_store = rustls::RootCertStore::empty();
        
        if let Some(ca_cert_pem) = &config.ca_cert_pem {
            // 如果提供了自定义 CA 证书，使用它
            let cert_bytes = ca_cert_pem.as_bytes();
            let ca_certs = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_bytes))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| QuicError::Config(format!("Failed to parse CA certificate: {}", e)))?;
            
            for cert in ca_certs {
                root_store.add(cert)
                    .map_err(|e| QuicError::Config(format!("Failed to add CA certificate to store: {}", e)))?;
            }
            
            tracing::debug!("🔐 使用自定义 CA 证书进行 QUIC 客户端证书验证");
        } else {
            // 使用系统根证书
            root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            tracing::debug!("🔐 使用系统根证书进行 QUIC 客户端证书验证");
        }
        
        rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth()
    } else {
        // 不验证证书（不安全模式）
        tracing::debug!("🔓 QUIC 客户端使用不安全模式（跳过证书验证）");
        rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
            .with_no_client_auth()
    };

    let mut client_config = ClientConfig::new(
        Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(crypto)
            .map_err(|e| QuicError::Config(format!("QUIC client config error: {}", e)))?)
    );
    
    // 配置传输参数
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(config.max_idle_timeout.try_into()
        .map_err(|e| QuicError::Config(format!("Invalid idle timeout: {}", e)))?));
    
    if let Some(keep_alive) = config.keep_alive_interval {
        transport_config.keep_alive_interval(Some(keep_alive));
    }
    
    transport_config.initial_rtt(config.initial_rtt);
    
    // Convert u64 to u32 for VarInt (VarInt only supports From<u32>)
    let max_streams = config.max_concurrent_streams.min(u32::MAX as u64) as u32;
    transport_config.max_concurrent_uni_streams(max_streams.into());
    transport_config.max_concurrent_bidi_streams(max_streams.into());
    
    client_config.transport_config(Arc::new(transport_config));
    
    Ok(client_config)
}

/// Configure server with self-signed certificate
fn configure_server_insecure_with_config(config: &QuicServerConfig) -> (ServerConfig, CertificateDer<'static>) {
    let (cert, key) = generate_self_signed_cert();
    
    let mut server_config = ServerConfig::with_single_cert(
        vec![cert.clone()], 
        key.into()
    ).unwrap();

    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.receive_window((1500u32 * 100).into());
    transport_config.max_idle_timeout(Some(config.max_idle_timeout.try_into().unwrap()));

    (server_config, cert)
}

/// Configure server with self-signed certificate (legacy function for backward compatibility)
fn configure_server_insecure() -> (ServerConfig, CertificateDer<'static>) {
    let default_config = QuicServerConfig::default();
    configure_server_insecure_with_config(&default_config)
}

/// Configure server with PEM certificate and key
fn configure_server_with_pem(cert_pem: &str, key_pem: &str, config: &QuicServerConfig) -> Result<(ServerConfig, CertificateDer<'static>), QuicError> {
    // Parse private key from PEM string
    let key_bytes = key_pem.as_bytes();
    let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_bytes))
        .map_err(|e| QuicError::Config(format!("Failed to parse private key: {}", e)))?
        .ok_or_else(|| QuicError::Config("No private key found in PEM data".to_string()))?;

    // Parse certificate chain from PEM string
    let cert_bytes = cert_pem.as_bytes();
    let certs = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_bytes))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| QuicError::Config(format!("Failed to parse certificates: {}", e)))?;
    
    if certs.is_empty() {
        return Err(QuicError::Config("No certificates found in PEM data".to_string()));
    }

    // Get the first certificate for return (client verification)
    let first_cert = certs[0].clone();

    // Create server crypto configuration
    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| QuicError::Config(format!("TLS configuration error: {}", e)))?;
    
    // Create QUIC server configuration
    let quic_server_config = quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
        .map_err(|e| QuicError::Config(format!("QUIC configuration error: {}", e)))?;
    
    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));
    
    // Configure transport parameters
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.receive_window((1500u32 * 100).into());
    transport_config.max_idle_timeout(Some(config.max_idle_timeout.try_into().unwrap()));
    
    Ok((server_config, first_cert))
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
                                        tracing::debug!("📥 QUIC接收到流数据: {} bytes (会话: {})", buf.len(), current_session_id);
                                        
                                        // ✅ 优化：QUIC流保证数据完整性，预检查避免无效解析
                                        let packet = if buf.len() < 16 {
                                            // 数据太短，不可能是有效的Packet，直接创建基本数据包
                                            tracing::debug!("📥 QUIC数据太短，创建基本数据包: {} bytes", buf.len());
                                            Packet::one_way(0, buf)
                                        } else {
                                            // 尝试解析为完整的Packet
                                            match Packet::from_bytes(&buf) {
                                                Ok(packet) => {
                                                    tracing::debug!("📥 QUIC解析数据包成功: {} bytes", packet.payload.len());
                                                    packet
                                                }
                                                Err(e) => {
                                                    tracing::debug!("📥 QUIC数据包解析失败: {:?}, 创建基本数据包", e);
                                                    // ✅ 优化：避免切片拷贝，直接使用buf
                                                    Packet::one_way(0, buf)
                                                }
                                            }
                                        };
                                        
                                        // 发送接收事件
                                        let event = TransportEvent::MessageReceived(packet);
                                        
                                        if let Err(e) = event_sender.send(event) {
                                            tracing::warn!("📥 发送接收事件失败: {:?}", e);
                                        }
                                    }
                                    Err(e) => {
                                        // ✅ 优化：更精细的QUIC错误分类处理
                                        let (should_notify, reason, log_level) = match e {
                                            quinn::ReadToEndError::Read(quinn::ReadError::ConnectionLost(_)) => {
                                                (true, crate::error::CloseReason::Normal, "debug")
                                            }
                                            quinn::ReadToEndError::Read(quinn::ReadError::Reset(_)) => {
                                                (true, crate::error::CloseReason::Normal, "debug")
                                            }
                                            quinn::ReadToEndError::TooLong => {
                                                (true, crate::error::CloseReason::Error("QUIC stream too long".to_string()), "warn")
                                            }
                                            _ => {
                                                (true, crate::error::CloseReason::Error(format!("QUIC stream error: {:?}", e)), "error")
                                            }
                                        };
                                        
                                        // ✅ 优化：更详细的日志记录
                                        match log_level {
                                            "debug" => tracing::debug!("📥 QUIC流正常关闭: {:?} (会话: {})", e, current_session_id),
                                            "warn" => tracing::warn!("📥 QUIC流警告: {:?} (会话: {})", e, current_session_id),
                                            "error" => tracing::error!("📥 QUIC流错误: {:?} (会话: {})", e, current_session_id),
                                            _ => {}
                                        }
                                        
                                        // 通知上层连接关闭（网络异常或对端关闭）
                                        if should_notify {
                                            let close_event = TransportEvent::ConnectionClosed { reason };
                                            
                                            if let Err(e) = event_sender.send(close_event) {
                                                tracing::debug!("🔗 通知上层连接关闭失败: 会话 {} - {:?}", current_session_id, e);
                                            } else {
                                                tracing::debug!("📡 已通知上层连接关闭: 会话 {}", current_session_id);
                                            }
                                        }
                                        
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                // ✅ 优化：增强QUIC连接错误分类
                                let (should_notify, reason, log_level) = match e {
                                    quinn::ConnectionError::TimedOut => {
                                        (true, crate::error::CloseReason::Timeout, "info")
                                    }
                                    quinn::ConnectionError::ConnectionClosed(_) => {
                                        (true, crate::error::CloseReason::Normal, "debug")
                                    }
                                    quinn::ConnectionError::ApplicationClosed(_) => {
                                        (true, crate::error::CloseReason::Normal, "debug")
                                    }
                                    quinn::ConnectionError::Reset => {
                                        (true, crate::error::CloseReason::Normal, "debug")
                                    }
                                    quinn::ConnectionError::LocallyClosed => {
                                        (false, crate::error::CloseReason::Normal, "debug") // 本地关闭不需要通知
                                    }
                                    _ => {
                                        (true, crate::error::CloseReason::Error(format!("QUIC connection error: {:?}", e)), "error")
                                    }
                                };
                                
                                // ✅ 优化：更精确的日志级别
                                match log_level {
                                    "debug" => tracing::debug!("📥 QUIC连接正常关闭: {:?} (会话: {})", e, current_session_id),
                                    "info" => tracing::info!("📥 QUIC连接超时: {:?} (会话: {})", e, current_session_id),
                                    "error" => tracing::error!("📥 QUIC连接错误: {:?} (会话: {})", e, current_session_id),
                                    _ => {}
                                }
                                
                                // 通知上层连接关闭（网络异常或对端关闭）
                                if should_notify {
                                    let close_event = TransportEvent::ConnectionClosed { reason };
                                    
                                    if let Err(e) = event_sender.send(close_event) {
                                        tracing::debug!("🔗 通知上层连接关闭失败: 会话 {} - {:?}", current_session_id, e);
                                    } else {
                                        tracing::debug!("📡 已通知上层连接关闭: 会话 {}", current_session_id);
                                    }
                                } else {
                                    tracing::debug!("🔌 本地关闭，无需通知上层 (会话: {})", current_session_id);
                                }
                                
                                break;
                            }
                        }
                    }
                    
                    // 📤 处理发送数据 - 优化版本
                    packet = send_queue.recv() => {
                        if let Some(packet) = packet {
                            match connection.open_uni().await {
                                Ok(mut send_stream) => {
                                    // ✅ 优化：准备发送数据
                                    let data = packet.to_bytes();
                                    let packet_size = packet.payload.len();
                                    let packet_id = packet.header.message_id;
                                    
                                    match send_stream.write_all(&data).await {
                                        Ok(_) => {
                                            // ✅ 优化：使用更高效的流关闭方式
                                            match send_stream.finish() {
                                                Ok(_) => {
                                                    tracing::debug!("📤 QUIC发送成功: {} bytes (ID: {}, 会话: {})", 
                                                        packet_size, packet_id, current_session_id);
                                                    
                                                    // 发送发送事件
                                                    let event = TransportEvent::MessageSent { packet_id };
                                                    
                                                    if let Err(e) = event_sender.send(event) {
                                                        tracing::warn!("📤 发送发送事件失败: {:?}", e);
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!("📤 QUIC流关闭错误: {:?} (会话: {})", e, current_session_id);
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
                        // 主动关闭：不需要发送关闭事件，因为是上层主动发起的关闭
                        // 底层协议关闭已经通知了对端，上层也已经知道要关闭了
                        tracing::debug!("🔌 主动关闭，不发送关闭事件");
                        break;
                    }
                }
            }
            
            tracing::debug!("✅ QUIC事件循环已结束 (会话: {})", current_session_id);
        })
    }
}

// 客户端适配器实现
impl QuicAdapter<QuicClientConfig> {
    /// 连接到QUIC服务器
    pub async fn connect(addr: SocketAddr, config: QuicClientConfig) -> Result<Self, QuicError> {
        tracing::debug!("🔌 QUIC客户端连接到: {}", addr);
        
        // 根据配置创建客户端配置
        let client_config = configure_client_with_config(&config)?;
        let mut endpoint = Endpoint::client(config.local_bind_address.unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 0))))?;
        endpoint.set_default_client_config(client_config);
        
        // 使用配置的服务器名称或默认值
        let server_name = config.server_name.as_deref().unwrap_or("localhost");
        
        // 连接到服务器（使用配置的超时时间）
        let connecting = endpoint.connect(addr, server_name)?;
        let connection = tokio::time::timeout(config.connect_timeout, connecting)
            .await
            .map_err(|_| QuicError::Config(format!("Connection timeout after {:?}", config.connect_timeout)))?
            .map_err(QuicError::Connection)?;
        tracing::debug!("✅ QUIC客户端已连接到: {} (服务器名称: {}) 超时: {:?}", addr, server_name, config.connect_timeout);
        
        Self::new_with_connection(connection, config, broadcast::channel(1000).0).await
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
        
        // 根据配置选择证书模式
        let server_config = match (&self.config.cert_pem, &self.config.key_pem) {
            (Some(cert_pem), Some(key_pem)) if !cert_pem.is_empty() && !key_pem.is_empty() => {
                // 使用传入的 PEM 证书和私钥
                tracing::debug!("🔐 使用传入的 PEM 证书启动 QUIC 服务器");
                let (server_config, _cert) = configure_server_with_pem(cert_pem, key_pem, &self.config)?;
                server_config
            }
            _ => {
                // 使用自签名证书
                tracing::debug!("🔓 使用自签名证书启动 QUIC 服务器");
                let (server_config, _cert) = configure_server_insecure_with_config(&self.config);
                server_config
            }
        };
        
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
        
        QuicAdapter::new_with_connection(connection, self.config.clone(), broadcast::channel(1000).0).await
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