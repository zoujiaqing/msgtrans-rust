/// 多协议Echo服务器 - 支持TCP、WebSocket、QUIC
use anyhow::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_hdr_async, tungstenite::{self, Message}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use futures::{SinkExt, StreamExt};
use std::time::Duration;
use std::sync::Arc;
use std::convert::TryInto;


// QUIC specific imports
use rcgen;
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};

#[tokio::main]
async fn main() -> Result<()> {
    // 安装默认的crypto provider
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install crypto provider"))?;
    
    println!("🌟 多协议Echo服务器");
    println!("===================");
    
    // 启动TCP Echo服务器 (端口 8001)
    tokio::spawn(async move {
        println!("启动 TCP Echo 服务器，监听端口 8001...");
        let listener = TcpListener::bind("127.0.0.1:8001").await.unwrap();
        println!("✅ TCP服务器启动成功: 127.0.0.1:8001");
        
        while let Ok((stream, addr)) = listener.accept().await {
            println!("TCP 新连接: {}", addr);
            tokio::spawn(handle_tcp_connection(stream));
        }
    });
    
    // 启动WebSocket Echo服务器 (端口 8002)
    tokio::spawn(async move {
        println!("启动 WebSocket Echo 服务器，监听端口 8002...");
        let listener = TcpListener::bind("127.0.0.1:8002").await.unwrap();
        println!("✅ WebSocket服务器启动成功: 127.0.0.1:8002");
        
        while let Ok((stream, addr)) = listener.accept().await {
            println!("WebSocket 新连接: {}", addr);
            tokio::spawn(handle_websocket_connection(stream));
        }
    });
    
    // 启动QUIC Echo服务器 (端口 8003)
    tokio::spawn(async move {
        println!("启动 QUIC Echo 服务器，监听端口 8003...");
        
        let server_config = configure_quic_server_insecure().unwrap();
        let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:8003".parse().unwrap()).unwrap();
        
        println!("✅ QUIC服务器启动成功: {}", endpoint.local_addr().unwrap());
        
        while let Some(incoming) = endpoint.accept().await {
            match incoming.await {
                Ok(connection) => {
                    println!("QUIC 新连接: {}", connection.remote_address());
                    tokio::spawn(handle_quic_connection(connection));
                },
                Err(e) => {
                    eprintln!("QUIC 连接失败: {}", e);
                }
            }
        }
    });
    
    println!("\n🎯 测试方法:");
    println!("   TCP:       cargo run --example echo_client_tcp");
    println!("   WebSocket: cargo run --example echo_client_websocket");
    println!("   QUIC:      cargo run --example echo_client_quic");
    println!("   Telnet:    telnet 127.0.0.1 8001");
    println!("\n按 Ctrl+C 停止服务器");
    
    // 等待Ctrl+C
    tokio::signal::ctrl_c().await?;
    println!("\n收到停止信号，关闭服务器...");
    
    Ok(())
}

async fn handle_tcp_connection(mut stream: TcpStream) -> Result<()> {
    let mut buffer = [0; 1024];
    
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) => break, // 连接关闭
            Ok(n) => {
                let data = &buffer[..n];
                let text = String::from_utf8_lossy(data);
                println!("TCP 收到: {}", text.trim());
                
                // 回显
                if let Err(e) = stream.write_all(data).await {
                    eprintln!("TCP 写入失败: {}", e);
                    break;
                }
                if let Err(e) = stream.flush().await {
                    eprintln!("TCP 刷新失败: {}", e);
                    break;
                }
                println!("TCP 已回显: {} 字节", n);
            }
            Err(e) => {
                eprintln!("TCP 读取失败: {}", e);
                break;
            }
        }
    }
    
    Ok(())
}

async fn handle_websocket_connection(stream: TcpStream) -> Result<()> {
    // 使用回调函数处理HTTP请求头，验证路径
    let callback = |req: &tungstenite::handshake::server::Request, 
                    response: tungstenite::handshake::server::Response| {
        println!("WebSocket握手请求路径: {}", req.uri().path());
        
        // 检查路径是否正确
        if req.uri().path() == "/echo" {
            Ok(response)
        } else {
            println!("WebSocket路径不匹配，期望 '/echo'，实际: '{}'", req.uri().path());
            Err(tungstenite::handshake::server::ErrorResponse::new(Some("路径不匹配".to_string())))
        }
    };
    
    // 使用带回调函数的accept_hdr_async进行握手
    let ws_stream = tokio_tungstenite::accept_hdr_async(stream, callback).await?;
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    println!("WebSocket连接建立成功");
    
    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("WebSocket 收到文本: {}", text);
                if let Err(e) = ws_sender.send(Message::Text(text)).await {
                    eprintln!("WebSocket 发送失败: {}", e);
                    break;
                }
                println!("WebSocket 已回显");
            }
            Ok(Message::Binary(data)) => {
                println!("WebSocket 收到二进制: {} 字节", data.len());
                if let Err(e) = ws_sender.send(Message::Binary(data)).await {
                    eprintln!("WebSocket 发送失败: {}", e);
                    break;
                }
                println!("WebSocket 已回显");
            }
            Ok(Message::Close(frame)) => {
                println!("WebSocket 收到关闭帧: {:?}", frame);
                // 正确响应关闭帧
                let _ = ws_sender.send(Message::Close(frame)).await;
                break;
            }
            Ok(Message::Ping(data)) => {
                println!("WebSocket 收到Ping");
                // 响应Pong
                let _ = ws_sender.send(Message::Pong(data)).await;
            }
            Ok(Message::Pong(_)) => {
                println!("WebSocket 收到Pong");
            }
            Err(e) => {
                eprintln!("WebSocket 错误: {}", e);
                break;
            }
            _ => {}
        }
    }
    
    println!("WebSocket 连接结束");
    Ok(())
}

async fn handle_quic_connection(connection: quinn::Connection) -> Result<()> {
    let remote_addr = connection.remote_address();
    println!("处理 QUIC 连接: {}", remote_addr);
    
    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
        println!("QUIC 新数据流来自: {}", remote_addr);
        
        let mut buffer = [0u8; 1024];
        while let Ok(Some(len)) = recv.read(&mut buffer).await {
            let message = &buffer[..len];
            let text = String::from_utf8_lossy(message);
            println!("QUIC 收到来自 {}: {}", remote_addr, text);
            
            // 发送回显
            if let Err(e) = send.write_all(message).await {
                eprintln!("QUIC 发送失败: {}", e);
                break;
            }
            if let Err(e) = send.flush().await {
                eprintln!("QUIC 刷新失败: {}", e);
                break;
            }
            println!("QUIC 已回显给 {}: {} 字节", remote_addr, len);
        }
    }
    
    println!("QUIC 连接关闭: {}", remote_addr);
    Ok(())
}

// 生成自签名证书
fn generate_self_signed_cert() -> Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)> {
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let cert = rcgen::generate_simple_self_signed(subject_alt_names)?;
    
    Ok((
        cert.cert.der().clone(),
        PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()),
    ))
}

// 配置QUIC服务器（非安全模式）
fn configure_quic_server_insecure() -> Result<quinn::ServerConfig> {
    let (cert, key) = generate_self_signed_cert()?;
    
    let mut server_config = quinn::ServerConfig::with_single_cert(vec![cert], key.into())?;
    
    // 配置传输参数
    let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
    transport_config.receive_window((1500u32 * 100).into());
    transport_config.max_idle_timeout(Some(Duration::from_secs(20).try_into().unwrap()));
    
    Ok(server_config)
}
