/// 多协议Echo服务器 - 使用标准Packet接口的最终版本
use anyhow::Result;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_hdr_async, tungstenite::{self, Message}};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use futures::{SinkExt, StreamExt};
use std::time::Duration;

// 使用msgtrans的标准接口 - 只与Packet交互
use msgtrans::{
    protocol::{QuicConfig, ProtocolAdapter},
    adapters::quic::QuicServerBuilder,
    packet::{Packet, PacketType},
};
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🌟 多协议Echo服务器 (标准Packet接口版)");
    println!("===================================");
    
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
    
    // 启动QUIC Echo服务器 (端口 8003) - 使用标准Packet接口
    tokio::spawn(async move {
        println!("启动 QUIC Echo 服务器，监听端口 8003...");
        
        // 使用QuicConfig API，自动生成自签名证书
        let config = QuicConfig::new("127.0.0.1:8003")
            .unwrap()
            .with_max_idle_timeout(Duration::from_secs(30))
            .with_max_concurrent_streams(100);
        
        let mut server = QuicServerBuilder::new()
            .config(config)
            .build()
            .await
            .unwrap();
        
        println!("✅ QUIC服务器启动成功: {}", server.local_addr().unwrap());
        
        while let Ok(connection) = server.accept().await {
            let remote_addr = connection.connection_info().peer_addr;
            println!("QUIC 新连接: {}", remote_addr);
            tokio::spawn(handle_quic_connection(connection));
        }
    });
    
    println!("\n🎯 测试方法:");
    println!("   TCP:       cargo run --example echo_client_tcp");
    println!("   WebSocket: cargo run --example echo_client_websocket");
    println!("   QUIC:      cargo run --example echo_client_quic_final");
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

// 使用标准Packet接口处理QUIC连接（应用层echo业务逻辑）
async fn handle_quic_connection(mut connection: msgtrans::adapters::quic::QuicAdapter) -> Result<()> {
    let remote_addr = connection.connection_info().peer_addr;
    println!("处理 QUIC 连接: {}", remote_addr);
    
    // 应用层echo业务逻辑 - 只使用标准的Packet接口
    while connection.is_connected() {
        match connection.receive().await {
            Ok(Some(packet)) => {
                let message = String::from_utf8_lossy(&packet.payload);
                println!("QUIC 收到来自 {}: {}", remote_addr, message);
                
                // 创建回显数据包
                let echo_packet = Packet {
                    packet_type: PacketType::Data,
                    message_id: packet.message_id, // 保持相同的消息ID
                    payload: packet.payload, // 直接回显原始数据
                };
                
                // 发送回显
                if let Err(e) = connection.send(echo_packet).await {
                    eprintln!("QUIC 发送失败: {}", e);
                    break;
                }
                
                println!("QUIC 已回显给: {}", remote_addr);
            }
            Ok(None) => {
                println!("QUIC 连接 {} 已关闭", remote_addr);
                break;
            }
            Err(e) => {
                eprintln!("QUIC 接收错误: {}", e);
                break;
            }
        }
    }
    
    println!("QUIC 连接 {} 处理结束", remote_addr);
    Ok(())
} 