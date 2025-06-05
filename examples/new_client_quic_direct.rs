use msgtrans::transport::{QuicConnection, ProtocolSpecific};
use msgtrans::packet::{Packet, PacketHeader};
use msgtrans::compression::CompressionMethod;
use tokio::io::{self, AsyncBufReadExt, BufReader};
use quinn::{ClientConfig, Endpoint};
use std::sync::Arc;
use std::net::SocketAddr;
use rustls::{ClientConfig as RustlsClientConfig, pki_types::{CertificateDer, ServerName, UnixTime}, 
            client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier}, 
            DigitallySignedStruct, SignatureScheme};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 新架构 QUIC 客户端测试");

    // 创建 Quinn 客户端配置（跳过证书验证）
    let mut rustls_config = RustlsClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerifier))
        .with_no_client_auth();
    
    rustls_config.alpn_protocols = vec![b"hq-29".to_vec()];
    let client_config = ClientConfig::new(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config)?));

    let mut endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
    endpoint.set_default_client_config(client_config);

    // 连接到QUIC服务器
    let server_addr: SocketAddr = "127.0.0.1:9003".parse()?;
    println!("📡 正在连接到QUIC服务器: {}", server_addr);
    
    let connecting = endpoint.connect(server_addr, "localhost")?;
    let quinn_connection = connecting.await?;
    println!("✅ QUIC连接已建立！");

    // 创建新架构的QUIC连接
    let quic_connection = QuicConnection::new(quinn_connection, 1).await?;
    println!("🔗 QuicConnection已创建！");

    // 获取接收器和发送器
    let receiver = quic_connection.create_receiver();
    let mut sender = quic_connection.create_sender();

    // 启动接收任务（直接使用event_rx，避免Stream问题）
    tokio::spawn(async move {
        println!("🚀 QUIC直接接收任务已启动...");
        // 注意：这里需要访问receiver的内部event_rx，
        // 但是QuicReceiver的实现可能不同，让我们先试试
        let mut receiver = receiver;
        use futures::StreamExt;
        while let Some(result) = receiver.next().await {
            match result {
                Ok(packet) => {
                    println!(
                        "🎉 收到QUIC服务器回复! ID: {}, 载荷: {:?}",
                        packet.header.message_id,
                        String::from_utf8_lossy(&packet.payload)
                    );
                }
                Err(e) => {
                    eprintln!("❌ QUIC接收错误: {:?}", e);
                    break;
                }
            }
        }
        println!("QUIC接收任务结束");
    });

    println!("📝 客户端配置完成！");

    // 发送初始消息
    use futures::SinkExt;
    
    let packet1 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello QUIC Server1!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello QUIC Server1!".as_bytes().to_vec(),
    );
    sender.send(packet1).await?;
    println!("📤 已发送消息1");

    let packet2 = Packet::new(
        PacketHeader {
            message_id: 2,
            message_length: "Hello QUIC Server2!".len() as u32,
            compression_type: CompressionMethod::None,
            extend_length: 0,
        },
        vec![],
        "Hello QUIC Server2!".as_bytes().to_vec(),
    );
    sender.send(packet2).await?;
    println!("📤 已发送消息2");

    // 处理用户输入
    let stdin = BufReader::new(io::stdin());
    let mut lines = stdin.lines();

    println!("💬 请输入消息（Ctrl+C 退出）:");
    while let Ok(Some(line)) = lines.next_line().await {
        let packet = Packet::new(
            PacketHeader {
                message_id: 2,
                message_length: line.len() as u32,
                compression_type: CompressionMethod::None,
                extend_length: 0,
            },
            vec![],
            line.as_bytes().to_vec(),
        );
        
        if let Err(e) = sender.send(packet).await {
            eprintln!("Failed to send packet: {}", e);
            break;
        }
    }

    Ok(())
}

/// 跳过服务器证书验证的结构体（仅用于测试）
#[derive(Debug)]
struct NoVerifier;

impl ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
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
            SignatureScheme::RSA_PKCS1_SHA1,
            SignatureScheme::ECDSA_SHA1_Legacy,
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
            SignatureScheme::ED448,
        ]
    }
}

