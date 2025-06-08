use async_trait::async_trait;
use std::{io, sync::Arc, time::Duration};
use bytes::BytesMut;
use quinn::{Endpoint, Connection, ClientConfig, ServerConfig, SendStream};
use rustls::{
    pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    DigitallySignedStruct, SignatureScheme,
};
use tokio::io::{AsyncWriteExt};

use crate::{
    SessionId, 
    protocol::{ProtocolAdapter, AdapterStats, QuicConfig},
    command::{ConnectionInfo, ProtocolType, ConnectionState},
    error::TransportError,
    packet::{Packet, PacketType},
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
    
    #[error("Quinn error: {0}")]
    Quinn(#[from] quinn::ConnectionError),
    
    #[error("QUIC endpoint error")]
    EndpointGeneric,
    
    #[error("QUIC connect error")]
    ConnectGeneric,
}

impl From<QuicError> for TransportError {
    fn from(error: QuicError) -> Self {
        match error {
            QuicError::Connection(msg) => TransportError::connection_error(msg, true),
            QuicError::Io(io_err) => TransportError::connection_error(format!("IO error: {:?}", io_err), true),
            QuicError::ConnectionClosed => TransportError::connection_error("Connection closed", true),
            QuicError::Stream(msg) => TransportError::protocol_error("generic", format!("Stream error: {}", msg)),
            QuicError::Certificate(msg) => TransportError::protocol_error("auth", msg),
            QuicError::Serialization(msg) => TransportError::protocol_error("serialization", msg),
            QuicError::Quinn(e) => TransportError::connection_error(format!("Quinn connection error: {}", e), true),
            QuicError::EndpointGeneric => TransportError::connection_error("Quinn endpoint error", true),
            QuicError::ConnectGeneric => TransportError::connection_error("Quinn connect error", true),
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
        _now: UnixTime,
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
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
        ]
    }
}

/// 配置不安全的QUIC客户端（跳过证书验证）
pub(crate) fn configure_client_insecure() -> ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    let mut client_config = ClientConfig::new(Arc::new(
        quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap()
    ));
    
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    client_config.transport_config(Arc::new(transport_config));
    
    client_config
}

// 静态变量存储证书，确保每次生成相同的证书
static mut CERT: Option<(Vec<u8>, Vec<u8>)> = None;

/// 配置QUIC服务器（自签名证书）
pub(crate) fn configure_server(recv_window_size: u32) -> (ServerConfig, CertificateDer<'static>) {
    
    // 使用静态变量存储证书，确保每次生成相同的证书
    let (our_cert, our_priv_key) = unsafe {
        if CERT.is_none() {
            let (cert, key) = gen_cert();
            let cert_bytes = cert.as_ref().to_vec();
            let key_bytes = key.secret_pkcs8_der().to_vec();
            CERT = Some((cert_bytes, key_bytes));
            (cert, key)
        } else {
            let (cert_bytes, key_bytes) = CERT.as_ref().unwrap();
            (
                CertificateDer::from(cert_bytes.clone()),
                PrivatePkcs8KeyDer::from(key_bytes.clone())
            )
        }
    };
    
    let mut our_cfg = ServerConfig::with_single_cert(
        vec![our_cert.clone()], 
        our_priv_key.into()
    ).unwrap();

    let transport_config = Arc::get_mut(&mut our_cfg.transport).unwrap();
    transport_config.receive_window(recv_window_size.into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));

    (our_cfg, our_cert)
}

fn gen_cert() -> (CertificateDer<'static>, PrivatePkcs8KeyDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    (
        cert.cert.der().clone(),
        PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
    )
}

/// 配置QUIC服务器（支持PEM内容或自动生成自签名证书）
pub(crate) fn configure_server_with_config(config: &QuicConfig) -> Result<(ServerConfig, Option<CertificateDer<'static>>), QuicError> {
    match (&config.cert_pem, &config.key_pem) {
        (Some(cert_pem), Some(key_pem)) => {
            // 使用提供的PEM证书
            configure_server_with_pem_content(cert_pem, key_pem)
        }
        (None, None) => {
            // 自动生成自签名证书
            let (server_config, cert) = configure_server(1500 * 100);
            Ok((server_config, Some(cert)))
        }
        _ => {
            // 不匹配的配置（验证阶段应该已经捕获）
            Err(QuicError::Certificate("证书和密钥必须同时提供或都不提供".to_string()))
        }
    }
}

/// 配置QUIC服务器（使用PEM内容）
pub(crate) fn configure_server_with_pem_content(
    cert_pem: &str,
    key_pem: &str,
) -> Result<(ServerConfig, Option<CertificateDer<'static>>), QuicError> {
    // 解析私钥
    let key = rustls_pemfile::private_key(&mut std::io::Cursor::new(key_pem))
        .map_err(|e| QuicError::Certificate(format!("解析私钥失败: {}", e)))?
        .ok_or_else(|| QuicError::Certificate("PEM中未找到私钥".to_string()))?;

    // 解析证书链
    let certs: Result<Vec<_>, _> = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_pem)).collect();
    let certs = certs.map_err(|e| QuicError::Certificate(format!("解析证书失败: {}", e)))?;
    
    if certs.is_empty() {
        return Err(QuicError::Certificate("PEM中未找到证书".to_string()));
    }

    // 创建服务器TLS配置
    let server_crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs.clone(), key)
        .map_err(|e| QuicError::Certificate(format!("TLS配置错误: {}", e)))?;
    
    // 创建QUIC服务器配置
    let quic_server_config = quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
        .map_err(|e| QuicError::Certificate(format!("QUIC配置错误: {}", e)))?;
    
    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_server_config));
    
    // 配置传输参数
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.receive_window((1500u32 * 100).into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    
    // 返回第一个证书用于客户端验证（如果需要）
    let first_cert = if certs.is_empty() { None } else { Some(certs[0].clone()) };
    
    Ok((server_config, first_cert))
}

/// 配置QUIC客户端（支持安全和非安全模式）
pub(crate) fn configure_client_with_config(config: &QuicConfig) -> ClientConfig {
    match (&config.cert_pem, &config.key_pem) {
        (Some(cert_pem), Some(_key_pem)) => {
            // 使用提供的证书进行服务器验证
            configure_client_with_pem_content(cert_pem)
        }
        _ => {
            // 非安全模式（跳过证书验证）
            configure_client_insecure()
        }
    }
}

/// 配置QUIC客户端（使用PEM证书进行服务器验证）
pub(crate) fn configure_client_with_pem_content(cert_pem: &str) -> ClientConfig {
    // 解析证书链
    let certs: Result<Vec<_>, _> = rustls_pemfile::certs(&mut std::io::Cursor::new(cert_pem)).collect();
    let certs = match certs {
        Ok(certs) if !certs.is_empty() => certs,
        _ => return configure_client_insecure(), // 解析失败时回退到非安全模式
    };
    
    // 配置信任存储
    let mut roots = rustls::RootCertStore::empty();
    for cert in certs {
        if roots.add(cert).is_err() {
            return configure_client_insecure(); // 添加失败时回退到非安全模式
        }
    }
    
    // 创建客户端TLS配置
    let client_crypto = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    
    // 创建QUIC客户端配置
    let quic_client_config = match quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto) {
        Ok(config) => config,
        Err(_) => return configure_client_insecure(), // 配置失败时回退到非安全模式
    };
    
    let mut client_config = ClientConfig::new(Arc::new(quic_client_config));
    
    // 配置传输参数
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    client_config.transport_config(Arc::new(transport_config));
    
    client_config
}

/// QUIC协议适配器
/// 
/// 使用真正的quinn库实现QUIC连接
pub struct QuicAdapter {
    /// 会话ID
    session_id: SessionId,
    /// 配置
    #[allow(dead_code)]
    config: QuicConfig,
    /// 统计信息
    stats: AdapterStats,
    /// 连接信息
    connection_info: ConnectionInfo,
    /// QUIC连接
    connection: Connection,
    /// 发送流
    send_stream: Option<SendStream>,
    /// 连接状态
    is_connected: bool,
}

impl QuicAdapter {
    /// 创建新的QUIC适配器（客户端模式）
    pub(crate) fn new(
        config: QuicConfig,
        connection: Connection,
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
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection_info,
            connection,
            send_stream: None,
            is_connected: true,
        }
    }
    
    /// 创建新的QUIC适配器（服务器端模式）
    pub(crate) fn new_server(
        config: QuicConfig,
        connection: Connection,
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
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection_info,
            connection,
            send_stream: None,
            is_connected: true,
        }
    }
    
    /// 连接到QUIC服务器（内部使用）
    pub(crate) async fn connect(
        addr: std::net::SocketAddr,
        config: QuicConfig,
    ) -> Result<Self, QuicError> {
        // 确保crypto provider已安装
        let _ = rustls::crypto::ring::default_provider().install_default();
        
        tracing::debug!("🔌 QUIC客户端连接到: {}", addr);
        
        // 使用新的配置函数（支持安全和非安全模式）
        let client_config = configure_client_with_config(&config);
        
        // 创建endpoint
        let local_addr = "0.0.0.0:0".parse().unwrap();
        let mut endpoint = Endpoint::client(local_addr)?;
        endpoint.set_default_client_config(client_config);
        
        // 连接到服务器
        let connection = endpoint.connect(addr, "localhost")
            .map_err(|e| QuicError::Connection(format!("Connect error: {}", e)))?
            .await?;
        
        tracing::debug!("✅ QUIC连接建立成功");
        
        Ok(Self::new(config, connection, local_addr, addr))
    }
    
    /// 发送原始数据（简化版本，不使用复杂的数据包格式）
    async fn send_raw(&mut self, data: &[u8]) -> Result<(), QuicError> {
        
        if !self.is_connected {
            return Err(QuicError::ConnectionClosed);
        }
        
        // 创建新的双向流进行发送（遵循QUIC最佳实践）
        let (mut send_stream, _) = self.connection.open_bi().await?;
        
        tracing::debug!("📤 QUIC发送原始数据: {} 字节", data.len());
        
                 // 发送数据
        send_stream.write_all(data).await.map_err(|e| QuicError::Stream(format!("Write error: {}", e)))?;
        send_stream.finish().map_err(|e| QuicError::Stream(format!("Finish error: {}", e)))?;
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
    /// 接收原始数据（简化版本，处理多个流）
    async fn receive_raw(&mut self) -> Result<Option<Vec<u8>>, QuicError> {
        
        if !self.is_connected {
            return Ok(None);
        }
        
        // 接受新的双向流
        match self.connection.accept_bi().await {
            Ok((_, mut recv_stream)) => {
                // 读取所有数据直到流结束（限制大小为1MB）
                match recv_stream.read_to_end(1024 * 1024).await {
                    Ok(buffer) => {
                        if buffer.is_empty() {
                            return Ok(None);
                        }
                        
                        tracing::debug!("📨 QUIC接收原始数据: {} 字节", buffer.len());
                        
                        // 记录统计信息
                        self.stats.record_packet_received(buffer.len());
                        self.connection_info.record_packet_received(buffer.len());
                        
                        Ok(Some(buffer))
                    }
                    Err(e) => Err(QuicError::Stream(format!("Read error: {}", e))),
                }
            }
            Err(quinn::ConnectionError::ApplicationClosed(_)) => {
                tracing::debug!("📡 QUIC连接已关闭");
                self.is_connected = false;
                Ok(None)
            }
            Err(e) => Err(QuicError::Stream(format!("Accept stream error: {}", e))),
        }
    }
    

}

#[async_trait]
impl ProtocolAdapter for QuicAdapter {
    type Config = QuicConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        // 使用新的简化发送方法
        self.send_raw(&packet.payload).await
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        // 使用新的简化接收方法
        match self.receive_raw().await? {
            Some(data) => {
                let packet = Packet {
                    packet_type: PacketType::Data,
                    message_id: 0, // 简化版本不使用消息ID
                    payload: BytesMut::from(&data[..]),
                };
                Ok(Some(packet))
            }
            None => Ok(None),
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            tracing::debug!("🔌 关闭QUIC连接");
            self.connection.close(0u32.into(), b"Normal closure");
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
        Ok(self.is_connected)
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        if let Some(ref mut send_stream) = self.send_stream {
            send_stream.flush().await.map_err(|e| QuicError::Stream(format!("Flush error: {}", e)))?;
        }
        Ok(())
    }
}

/// QUIC服务器构建器（内部使用）
pub(crate) struct QuicServerBuilder {
    config: QuicConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl QuicServerBuilder {
    /// 创建新的服务器构建器
    pub(crate) fn new() -> Self {
        Self {
            config: QuicConfig::default(),
            bind_address: None,
        }
    }
    
    /// 设置绑定地址
    pub(crate) fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    /// 设置配置
    pub(crate) fn config(mut self, config: QuicConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 构建服务器
    pub(crate) async fn build(self) -> Result<QuicServer, QuicError> {
        // 确保crypto provider已安装
        let _ = rustls::crypto::ring::default_provider().install_default();
        
        // 优先使用显式设置的bind_address，其次使用config中的bind_address
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        
        // 使用新的配置函数（支持PEM内容或自动生成自签名证书）
        let (server_config, _) = configure_server_with_config(&self.config)?;
        
        // 创建endpoint
        let endpoint = Endpoint::server(server_config, bind_addr)?;
        
        tracing::info!("🚀 QUIC服务器启动在: {}", endpoint.local_addr()?);
        
        Ok(QuicServer {
            endpoint,
            config: self.config,
        })
    }
}

impl Default for QuicServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// QUIC服务器（内部使用）
pub(crate) struct QuicServer {
    endpoint: Endpoint,
    config: QuicConfig,
}

impl QuicServer {

    
    /// 接受新连接
    pub(crate) async fn accept(&mut self) -> Result<QuicAdapter, QuicError> {
        if let Some(incoming) = self.endpoint.accept().await {
            let connection = incoming.await?;
            let local_addr = self.endpoint.local_addr()?;
            let peer_addr = connection.remote_address();
            
            tracing::debug!("🔗 QUIC新连接来自: {}", peer_addr);
            
            Ok(QuicAdapter::new_server(self.config.clone(), connection, local_addr, peer_addr))
        } else {
            Err(QuicError::ConnectionClosed)
        }
    }
    
    /// 获取本地地址
    pub(crate) fn local_addr(&self) -> Result<std::net::SocketAddr, QuicError> {
        Ok(self.endpoint.local_addr()?)
    }
}

/// QUIC客户端构建器（内部使用）
pub(crate) struct QuicClientBuilder {
    config: QuicConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl QuicClientBuilder {
    /// 创建新的客户端构建器
    pub(crate) fn new() -> Self {
        Self {
            config: QuicConfig::default(),
            target_address: None,
        }
    }
    
    /// 设置目标地址
    pub(crate) fn target_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.target_address = Some(addr);
        self
    }
    
    /// 设置配置
    pub(crate) fn config(mut self, config: QuicConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 连接到服务器
    pub(crate) async fn connect(self) -> Result<QuicAdapter, QuicError> {
        let target_addr = self.target_address.ok_or_else(|| {
            QuicError::Connection("No target address specified".to_string())
        })?;
        
        QuicAdapter::connect(target_addr, self.config).await
    }
}

impl Default for QuicClientBuilder {
    fn default() -> Self {
        Self::new()
    }
} 