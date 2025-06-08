/// 简单功能测试 - 调试QUIC连接问题
use msgtrans::{Builder, Config, Packet, TcpConfig, WebSocketConfig, QuicConfig};
use msgtrans::event::TransportEvent as Event;
use std::time::Duration;
use futures::StreamExt;
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("开始测试各协议的基本连接功能...");
    
    // 测试TCP
    println!("\n=== 测试TCP ===");
    match test_protocol("tcp").await {
        Ok(elapsed) => println!("TCP测试成功，耗时: {:?}", elapsed),
        Err(e) => println!("TCP测试失败: {}", e),
    }
    
    // 测试WebSocket
    println!("\n=== 测试WebSocket ===");
    match test_protocol("websocket").await {
        Ok(elapsed) => println!("WebSocket测试成功，耗时: {:?}", elapsed),
        Err(e) => println!("WebSocket测试失败: {}", e),
    }
    
    // 测试QUIC
    println!("\n=== 测试QUIC ===");
    match test_protocol("quic").await {
        Ok(elapsed) => println!("QUIC测试成功，耗时: {:?}", elapsed),
        Err(e) => println!("QUIC测试失败: {}", e),
    }
    
    Ok(())
}

async fn test_protocol(protocol: &str) -> Result<Duration, Box<dyn std::error::Error + Send + Sync>> {
    let addr = match protocol {
        "tcp" => "127.0.0.1:13001",
        "websocket" => "127.0.0.1:13002", 
        "quic" => "127.0.0.1:13003",
        _ => return Err("Unknown protocol".into()),
    };
    
    println!("启动{}服务器在地址: {}", protocol, addr);
    
    // 启动服务器
    let server_config = Config::default();
    let server_transport = Builder::new().config(server_config).build().await?;
    
    let _server = match protocol {
        "tcp" => {
            let tcp_config = TcpConfig::new(addr)?.with_nodelay(true);
            server_transport.listen(tcp_config).await?
        }
        "websocket" => {
            let ws_config = WebSocketConfig::new(addr)?.with_path("/test");
            server_transport.listen(ws_config).await?
        }
        "quic" => {
            let quic_config = QuicConfig::new(addr)?
                .with_max_idle_timeout(Duration::from_secs(30));
            server_transport.listen(quic_config).await?
        }
        _ => return Err("Unknown protocol".into()),
    };
    
    println!("{}服务器启动成功", protocol);
    
    // Echo服务器逻辑
    let server_transport_clone = server_transport.clone();
    tokio::spawn(async move {
        let mut events = server_transport_clone.events();
        while let Some(event) = events.next().await {
            match event {
                Event::MessageReceived { session_id, packet } => {
                    println!("服务器收到消息: id={}, payload_len={}", packet.message_id, packet.payload.len());
                    let echo_packet = Packet::data(packet.message_id, packet.payload);
                    if let Err(e) = server_transport_clone.send_to_session(session_id, echo_packet).await {
                        println!("服务器发送回复失败: {}", e);
                    }
                }
                Event::ConnectionEstablished { session_id, .. } => {
                    println!("新连接建立: session_id={}", session_id);
                }
                Event::ConnectionClosed { session_id, .. } => {
                    println!("连接断开: session_id={}", session_id);
                }
                _ => {}
            }
        }
    });
    
    // 等待服务器启动
    let wait_time = if protocol == "quic" { 500 } else { 200 };
    println!("等待{}ms让服务器完全启动...", wait_time);
    tokio::time::sleep(Duration::from_millis(wait_time)).await;
    
    // 客户端连接
    println!("创建{}客户端连接...", protocol);
    let client_config = Config::default();
    let client_transport = Builder::new().config(client_config).build().await?;
    
    let start = std::time::Instant::now();
    
    let session_id = match protocol {
        "tcp" => {
            let tcp_config = TcpConfig::new(addr)?;
            client_transport.connect(tcp_config).await?
        }
        "websocket" => {
            let ws_config = WebSocketConfig::new(addr)?.with_path("/test");
            client_transport.connect(ws_config).await?
        }
        "quic" => {
            let quic_config = QuicConfig::new(addr)?;
            client_transport.connect(quic_config).await?
        }
        _ => return Err("Unknown protocol".into()),
    };
    
    println!("{}客户端连接成功，session_id={}", protocol, session_id);
    
    // 发送测试消息
    let test_data = BytesMut::from("hello world".as_bytes());
    let packet = Packet::data(1, test_data);
    println!("发送测试消息...");
    client_transport.send_to_session(session_id, packet).await?;
    
    // 等待回复
    let mut client_events = client_transport.events();
    let timeout_duration = if protocol == "quic" { Duration::from_secs(15) } else { Duration::from_secs(10) };
    
    println!("等待服务器回复（超时时间: {:?}）...", timeout_duration);
    let timeout = tokio::time::timeout(timeout_duration, async {
        while let Some(event) = client_events.next().await {
            match event {
                Event::MessageReceived { packet, .. } => {
                    println!("客户端收到回复: id={}, payload_len={}", packet.message_id, packet.payload.len());
                    if packet.message_id == 1 {
                        return Ok(());
                    }
                }
                Event::ConnectionEstablished { session_id, .. } => {
                    println!("客户端连接确认: session_id={}", session_id);
                }
                Event::ConnectionClosed { session_id, .. } => {
                    println!("客户端连接断开: session_id={}", session_id);
                }
                _ => {}
            }
        }
        Err("No response received")
    });
    
    match timeout.await {
        Ok(Ok(())) => {
            let elapsed = start.elapsed();
            println!("{}测试完成，总耗时: {:?}", protocol, elapsed);
            Ok(elapsed)
        }
        Ok(Err(e)) => {
            println!("{}测试失败: {}", protocol, e);
            Err(e.into())
        }
        Err(_) => {
            println!("{}测试超时", protocol);
            Err("Timeout waiting for response".into())
        }
    }
} 