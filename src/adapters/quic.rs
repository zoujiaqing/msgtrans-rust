/// 🔧 事件驱动QUIC适配器
/// 
/// 这是QUIC适配器的现代化版本TODO，支持：
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
use tokio::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    SessionId, 
    error::TransportError, 
    packet::Packet, 
    command::ConnectionInfo,
    protocol::{ProtocolAdapter, AdapterStats, QuicClientConfig, QuicServerConfig},
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
    session_id: SessionId,
    config: C,
    stats: AdapterStats,
    connection: Option<Connection>,
    endpoint: Option<Endpoint>,
    is_connected: bool,
}

impl<C> QuicAdapter<C> {
    pub fn new(config: C) -> Self {
        Self {
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection: None,
            endpoint: None,
            is_connected: false,
        }
    }
    
    pub fn new_with_connection(config: C, connection: Connection) -> Self {
        Self {
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection: Some(connection),
            endpoint: None,
            is_connected: true,
        }
    }
    
    pub fn new_with_endpoint(config: C, endpoint: Endpoint) -> Self {
        Self {
            session_id: SessionId::new(0),
            config,
            stats: AdapterStats::new(),
            connection: None,
            endpoint: Some(endpoint),
            is_connected: false,
        }
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
        
        let mut adapter = Self::new(config);
        adapter.connection = Some(connection);
        adapter.endpoint = Some(endpoint);
        adapter.is_connected = true;
        
        Ok(adapter)
    }
}

#[async_trait]
impl ProtocolAdapter for QuicAdapter<QuicClientConfig> {
    type Config = QuicClientConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if let Some(ref connection) = self.connection {
            // 每次发送都创建新的单向流
            let mut send_stream = connection.open_uni().await?;
            let data = packet.to_bytes();
            send_stream.write_all(&data).await?;
            send_stream.finish()?;
            
            // 更新统计信息
            self.stats.record_packet_sent(packet.payload.len());
            Ok(())
        } else {
            Err(QuicError::ConnectionClosed)
        }
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if let Some(ref connection) = self.connection {
            // 等待接收流
            let mut recv_stream = connection.accept_uni().await?;
            let buf = recv_stream.read_to_end(1024 * 1024).await?; // 1MB limit
            
            // 尝试解析数据包
            match Packet::from_bytes(&buf) {
                Ok(packet) => {
                    self.stats.record_packet_received(buf.len());
                    Ok(Some(packet))
                }
                Err(_) => {
                    // 如果解析失败，创建基本数据包
                    let packet = Packet::data(0, &buf[..]);
                    self.stats.record_packet_received(buf.len());
                    Ok(Some(packet))
                }
            }
        } else {
            Err(QuicError::ConnectionClosed)
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            tracing::debug!("🔌 关闭QUIC连接");
            self.is_connected = false;
        }
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::default()
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

// 服务端适配器实现
#[async_trait]
impl ProtocolAdapter for QuicAdapter<QuicServerConfig> {
    type Config = QuicServerConfig;
    type Error = QuicError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        if let Some(ref connection) = self.connection {
            // 每次发送都创建新的单向流
            let mut send_stream = connection.open_uni().await?;
            let data = packet.to_bytes();
            send_stream.write_all(&data).await?;
            send_stream.finish()?;
            
            // 更新统计信息
            self.stats.record_packet_sent(packet.payload.len());
            Ok(())
        } else {
            Err(QuicError::ConnectionClosed)
        }
    }
    
    async fn receive(&mut self) -> Result<Option<Packet>, Self::Error> {
        if let Some(ref connection) = self.connection {
            // 等待接收流
            let mut recv_stream = connection.accept_uni().await?;
            let buf = recv_stream.read_to_end(1024 * 1024).await?; // 1MB limit
            
            // 尝试解析数据包
            match Packet::from_bytes(&buf) {
                Ok(packet) => {
                    self.stats.record_packet_received(buf.len());
                    Ok(Some(packet))
                }
                Err(_) => {
                    // 如果解析失败，创建基本数据包
                    let packet = Packet::data(0, &buf[..]);
                    self.stats.record_packet_received(buf.len());
                    Ok(Some(packet))
                }
            }
        } else {
            Err(QuicError::ConnectionClosed)
        }
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        if self.is_connected {
            tracing::debug!("🔌 关闭QUIC服务端连接");
            self.is_connected = false;
        }
        Ok(())
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::default()
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

/// QUIC服务器构建器
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
        let bind_addr = self.bind_address.unwrap_or(self.config.bind_address);
        
        tracing::debug!("🚀 QUIC服务器启动在: {}", bind_addr);
        
        // 配置服务器
        let (server_config, _cert) = configure_server_insecure();
        let endpoint = Endpoint::server(server_config, bind_addr)?;
        
        tracing::debug!("✅ QUIC服务器已启动在: {}", bind_addr);
        
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

/// QUIC服务器
pub(crate) struct QuicServer {
    config: QuicServerConfig,
    endpoint: Endpoint,
}

impl QuicServer {
    pub(crate) fn builder() -> QuicServerBuilder {
        QuicServerBuilder::new()
    }
    
    pub(crate) async fn accept(&mut self) -> Result<QuicAdapter<QuicServerConfig>, QuicError> {
        tracing::debug!("🔗 QUIC等待连接...");
        
        // 等待新连接
        let incoming = self.endpoint.accept().await
            .ok_or_else(|| QuicError::Config("No incoming connections".to_string()))?;
        
        let connection = incoming.await?;
        tracing::debug!("✅ QUIC新连接来自: {}", connection.remote_address());
        
        let adapter = QuicAdapter::new_with_connection(self.config.clone(), connection);
        Ok(adapter)
    }
    
    pub(crate) fn local_addr(&self) -> Result<SocketAddr, QuicError> {
        Ok(self.endpoint.local_addr()?)
    }
}

// 为示例提供的公共接口
#[cfg(any(test, feature = "examples"))]
impl QuicServer {
    /// 为示例创建服务器构建器
    pub fn example_builder() -> QuicServerBuilder {
        QuicServerBuilder::new()
    }
    
    /// 为示例接受连接
    pub async fn example_accept(&mut self) -> Result<QuicAdapter<QuicServerConfig>, QuicError> {
        self.accept().await
    }
    
    /// 为示例获取本地地址
    pub fn example_local_addr(&self) -> Result<SocketAddr, QuicError> {
        self.local_addr()
    }
}

/// QUIC客户端构建器
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
        let target_addr = self.target_address.unwrap_or(self.config.target_address);
        QuicAdapter::connect(target_addr, self.config).await
    }
}

impl Default for QuicClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}