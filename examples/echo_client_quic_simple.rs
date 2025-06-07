/// 简化的QUIC Echo客户端 - 直接使用quinn API
use std::time::Duration;
use anyhow::Result;
use quinn::{Endpoint, Connection};
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{pki_types::{CertificateDer, ServerName, UnixTime}, SignatureScheme, DigitallySignedStruct};
use std::sync::Arc;
use std::convert::TryInto;

// 自定义证书验证器，跳过服务器证书验证
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
            SignatureScheme::ED25519,
        ]
    }
}

// 配置QUIC客户端（非安全模式）
fn configure_quic_client_insecure() -> quinn::ClientConfig {
    let crypto = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

    let mut client_config = quinn::ClientConfig::new(
        Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(crypto).unwrap())
    );
    
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    client_config.transport_config(Arc::new(transport_config));
    
    client_config
}

#[tokio::main]
async fn main() -> Result<()> {
    // 安装默认的crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install crypto provider"))?;
    
    println!("🔧 简化QUIC Echo 客户端启动...");
    
    // 配置客户端
    let config = configure_quic_client_insecure();
    let mut endpoint = Endpoint::client(SocketAddr::from(([0, 0, 0, 0], 0)))?;
    endpoint.set_default_client_config(config);
    
    // 连接到服务器
    let server_addr: SocketAddr = "127.0.0.1:8003".parse()?;
    println!("🌐 连接到服务器: {}", server_addr);
    
    let connection = endpoint.connect(server_addr, "localhost")?.await?;
    println!("✅ 已连接到 QUIC 服务器");
    
    // 发送消息并接收回显
    let message = "Hello from simple QUIC client!";
    let echo = send_and_receive_echo(&connection, message).await?;
    
    println!("📤 发送: {}", message);
    println!("📥 回显: {}", echo);
    
    // 多次测试
    for i in 1..=3 {
        let test_message = format!("Simple test message #{}", i);
        let echo = send_and_receive_echo(&connection, &test_message).await?;
        println!("📤 发送: {}", test_message);
        println!("📥 回显: {}", echo);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    
    println!("🎯 简化QUIC 客户端测试完成");
    
    Ok(())
}

async fn send_and_receive_echo(connection: &Connection, message: &str) -> Result<String> {
    // 打开双向流
    let (mut send, mut recv) = connection.open_bi().await?;
    
    // 发送消息
    send.write_all(message.as_bytes()).await?;
    send.flush().await?;
    
    // 接收回显
    let mut buffer = [0u8; 1024];
    let len = recv.read(&mut buffer).await?
        .ok_or_else(|| anyhow::anyhow!("接收回显失败"))?;
    
    let echo = String::from_utf8_lossy(&buffer[..len]).to_string();
    Ok(echo)
} 