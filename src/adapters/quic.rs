use async_trait::async_trait;
use std::{io, sync::Arc, time::Duration};
use bytes::BytesMut;
use quinn::{Endpoint, Connection, ClientConfig, ServerConfig, RecvStream, SendStream};
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
            QuicError::Connection(msg) => TransportError::Connection(msg),
            QuicError::Io(io_err) => TransportError::Io(io_err),
            QuicError::ConnectionClosed => TransportError::Connection("Connection closed".to_string()),
            QuicError::Stream(msg) => TransportError::Protocol(format!("Stream error: {}", msg)),
            QuicError::Certificate(msg) => TransportError::Authentication(msg),
            QuicError::Serialization(msg) => TransportError::Serialization(msg),
            QuicError::Quinn(e) => TransportError::Connection(format!("Quinn connection error: {}", e)),
            QuicError::EndpointGeneric => TransportError::Connection("Quinn endpoint error".to_string()),
            QuicError::ConnectGeneric => TransportError::Connection("Quinn connect error".to_string()),
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
pub fn configure_client_insecure() -> ClientConfig {
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
pub fn configure_server(recv_window_size: u32) -> (ServerConfig, CertificateDer<'static>) {
    
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
pub fn configure_server_with_config(config: &QuicConfig) -> Result<(ServerConfig, Option<CertificateDer<'static>>), QuicError> {
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
pub fn configure_server_with_pem_content(
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
pub fn configure_client_with_config(config: &QuicConfig) -> ClientConfig {
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
pub fn configure_client_with_pem_content(cert_pem: &str) -> ClientConfig {
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
    /// 接收流
    recv_stream: Option<RecvStream>,
    /// 连接状态
    is_connected: bool,
    /// 是否为客户端模式
    is_client: bool,
}

impl QuicAdapter {
    /// 创建新的QUIC适配器（客户端模式）
    pub fn new(
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
            recv_stream: None,
            is_connected: true,
            is_client: true, // 默认为客户端模式
        }
    }
    
    /// 创建新的QUIC适配器（服务器端模式）
    pub fn new_server(
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
            recv_stream: None,
            is_connected: true,
            is_client: false, // 服务器端模式
        }
    }
    
    /// 连接到QUIC服务器
    pub async fn connect(
        addr: std::net::SocketAddr,
        config: QuicConfig,
    ) -> Result<Self, QuicError> {
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
    
    /// 确保双向流已开启
    async fn ensure_streams(&mut self) -> Result<(), QuicError> {
        if self.send_stream.is_none() || self.recv_stream.is_none() {
            if self.is_client {
                // 客户端模式：主动创建双向流
                let (send, recv) = self.connection.open_bi().await?;
                self.send_stream = Some(send);
                self.recv_stream = Some(recv);
                tracing::debug!("📡 QUIC双向流已建立（客户端模式）");
            } else {
                // 服务器端模式：等待并接受双向流
                match self.connection.accept_bi().await {
                    Ok((send, recv)) => {
                        self.send_stream = Some(send);
                        self.recv_stream = Some(recv);
                        tracing::debug!("📡 QUIC双向流已接受（服务器端模式）");
                    }
                    Err(e) => {
                        return Err(QuicError::Stream(format!("Accept stream error: {}", e)));
                    }
                }
            }
        }
        Ok(())
    }
    
    /// 序列化数据包
    fn serialize_packet(&self, packet: &Packet) -> Result<Vec<u8>, QuicError> {
        // 简单的序列化格式：[长度:4字节][类型:1字节][消息ID:4字节][负载]
        let mut buffer = Vec::new();
        let payload_len = packet.payload.len();
        
        if payload_len > u32::MAX as usize {
            return Err(QuicError::Serialization("Payload too large".to_string()));
        }
        
        buffer.extend_from_slice(&(payload_len as u32).to_be_bytes());
        buffer.push(packet.packet_type.into());
        buffer.extend_from_slice(&packet.message_id.to_be_bytes());
        buffer.extend_from_slice(&packet.payload);
        
        Ok(buffer)
    }
    
    /// 反序列化数据包
    fn deserialize_packet(&self, data: &[u8]) -> Result<Packet, QuicError> {
        if data.len() < 9 {
            return Err(QuicError::Serialization("Data too short".to_string()));
        }
        
        let payload_len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        
        if data.len() != payload_len + 9 {
            return Err(QuicError::Serialization("Invalid data length".to_string()));
        }
        
        let packet_type = data[4];
        let message_id = u32::from_be_bytes([data[5], data[6], data[7], data[8]]);
        let payload = data[9..].to_vec();
        
        Ok(Packet {
            packet_type: PacketType::from(packet_type),
            message_id,
            payload: BytesMut::from(&payload[..]),
        })
    }
}

#[async_trait]
impl ProtocolAdapter for QuicAdapter {
    type Config = QuicConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if !self.is_connected {
            return Err(QuicError::ConnectionClosed);
        }
        
        // 确保流已建立
        self.ensure_streams().await?;
        
        // 序列化数据包
        let data = self.serialize_packet(&packet)?;
        
        tracing::debug!("📤 QUIC发送数据包: 类型{:?}, ID{}, 大小{}字节", 
                       packet.packet_type, packet.message_id, data.len());
        
        // 发送数据
        if let Some(ref mut send_stream) = self.send_stream {
            send_stream.write_all(&data).await.map_err(|e| QuicError::Stream(format!("Write error: {}", e)))?;
            send_stream.flush().await.map_err(|e| QuicError::Stream(format!("Flush error: {}", e)))?;
        } else {
            return Err(QuicError::Stream("Send stream not available".to_string()));
        }
        
        // 记录统计信息
        self.stats.record_packet_sent(data.len());
        self.connection_info.record_packet_sent(data.len());
        
        Ok(())
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if !self.is_connected {
            return Ok(None);
        }
        
        // 确保流已建立
        self.ensure_streams().await?;
        
        if let Some(ref mut recv_stream) = self.recv_stream {
            // 读取包头（长度）
            let mut length_buf = [0u8; 4];
            match recv_stream.read_exact(&mut length_buf).await {
                Ok(()) => {},
                Err(quinn::ReadExactError::FinishedEarly(_)) => {
                    tracing::debug!("📡 QUIC连接关闭");
                    self.is_connected = false;
                    return Ok(None);
                },
                Err(e) => return Err(QuicError::Stream(format!("Read error: {}", e))),
            }
            
            let payload_len = u32::from_be_bytes(length_buf) as usize;
            
            // 读取剩余的包头和负载
            let mut packet_buf = vec![0u8; payload_len + 5]; // +5 for type and message_id
            match recv_stream.read_exact(&mut packet_buf).await {
                Ok(()) => {},
                Err(e) => return Err(QuicError::Stream(format!("Read packet error: {}", e))),
            }
            
            // 组合完整数据包
            let mut full_data = Vec::with_capacity(payload_len + 9);
            full_data.extend_from_slice(&length_buf);
            full_data.extend_from_slice(&packet_buf);
            
            // 反序列化数据包
            let packet = self.deserialize_packet(&full_data)?;
            
            tracing::debug!("📨 QUIC接收数据包: 类型{:?}, ID{}, 大小{}字节", 
                           packet.packet_type, packet.message_id, full_data.len());
            
            // 记录统计信息
            self.stats.record_packet_received(full_data.len());
            self.connection_info.record_packet_received(full_data.len());
            
            Ok(Some(packet))
        } else {
            Err(QuicError::Stream("Receive stream not available".to_string()))
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

/// QUIC服务器构建器
pub struct QuicServerBuilder {
    config: QuicConfig,
    bind_address: Option<std::net::SocketAddr>,
}

impl QuicServerBuilder {
    /// 创建新的服务器构建器
    pub fn new() -> Self {
        Self {
            config: QuicConfig::default(),
            bind_address: None,
        }
    }
    
    /// 设置绑定地址
    pub fn bind_address(mut self, addr: std::net::SocketAddr) -> Self {
        self.bind_address = Some(addr);
        self
    }
    
    /// 设置配置
    pub fn config(mut self, config: QuicConfig) -> Self {
        self.config = config;
        self
    }
    
    /// 构建服务器
    pub async fn build(self) -> Result<QuicServer, QuicError> {
        let bind_addr = self.bind_address.unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());
        
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

/// QUIC服务器
pub struct QuicServer {
    endpoint: Endpoint,
    config: QuicConfig,
}

impl QuicServer {
    /// 创建服务器构建器
    pub fn builder() -> QuicServerBuilder {
        QuicServerBuilder::new()
    }
    
    /// 接受新连接
    pub async fn accept(&mut self) -> Result<QuicAdapter, QuicError> {
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
    pub fn local_addr(&self) -> Result<std::net::SocketAddr, QuicError> {
        Ok(self.endpoint.local_addr()?)
    }
}

/// QUIC客户端构建器
pub struct QuicClientBuilder {
    config: QuicConfig,
    target_address: Option<std::net::SocketAddr>,
}

impl QuicClientBuilder {
    /// 创建新的客户端构建器
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
    
    /// 连接到服务器
    pub async fn connect(self) -> Result<QuicAdapter, QuicError> {
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