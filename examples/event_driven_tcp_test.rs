/// 事件驱动TCP通信测试
/// 
/// 验证基于 tokio::select! 的事件驱动架构是否正常工作

use msgtrans::{
    transport::{TransportServerBuilder, TransportClientBuilder},
    protocol::{TcpServerConfig, TcpClientConfig},
    event::TransportEvent,
    packet::{Packet, PacketType},
};
use futures::StreamExt;
use tokio::time::{sleep, Duration};
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🚀 事件驱动TCP通信测试");
    println!("===================");
    
    // 1. 启动服务端
    let server_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:9998".parse::<std::net::SocketAddr>()?);
        
    let server = TransportServerBuilder::new()
        .with_protocol(server_config)
        .build()
        .await?;
    
    println!("✅ 服务端启动: 127.0.0.1:9998");
    
    // 获取服务端事件流
    let server_clone = server.clone();
    let mut server_events = server_clone.events();
    
    // 启动服务端事件监听
    let server_task = tokio::spawn(async move {
        println!("🎧 服务端事件监听开始...");
        let mut connection_count: u32 = 0;
        let mut message_count = 0;
        
        while let Some(event) = server_events.next().await {
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    connection_count += 1;
                    println!("🔗 [服务端] 新连接: {} (总连接数: {})", session_id, connection_count);
                    println!("   地址: {} -> {}", info.peer_addr, info.local_addr);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    message_count += 1;
                    println!("📨 [服务端] 收到消息 #{}: {} bytes (会话: {})", 
                        message_count, packet.payload.len(), session_id);
                    
                    if let Some(text) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", text);
                        
                        // 发送回显
                        let echo_text = format!("Echo: {}", text);
                        let echo_packet = Packet::new(
                            PacketType::Data,
                            packet.message_id + 1000, // 不同的ID
                            BytesMut::from(echo_text.as_bytes()),
                        );
                        
                        // TODO: 这里需要实现服务端发送功能
                        println!("📤 [服务端] 准备发送回显: \"{}\"", echo_text);
                    }
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    connection_count = connection_count.saturating_sub(1);
                    println!("❌ [服务端] 连接关闭: {} (原因: {:?}, 剩余连接: {})", 
                        session_id, reason, connection_count);
                }
                _ => {
                    println!("ℹ️ [服务端] 其他事件: {:?}", event);
                }
            }
        }
        
        println!("⚠️ 服务端事件流结束");
    });
    
    // 启动服务端
    let server_serve_task = tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            eprintln!("❌ 服务端错误: {:?}", e);
        }
    });
    
    // 等待服务端启动
    sleep(Duration::from_millis(500)).await;
    
    // 2. 启动客户端
    let client_config = TcpClientConfig::new()
        .with_target_address("127.0.0.1:9998".parse::<std::net::SocketAddr>()?);
        
    let mut client = TransportClientBuilder::new()
        .with_protocol(client_config)
        .build()
        .await?;
    
    println!("✅ 客户端创建完成");
    
    // 连接到服务端
    let session_id = client.connect().await?;
    println!("✅ 客户端连接成功: {}", session_id);
    
    // 获取客户端事件流
    let mut client_events = client.events().await?;
    
    // 启动客户端事件监听
    let client_task = tokio::spawn(async move {
        println!("🎧 客户端事件监听开始...");
        let mut message_count = 0;
        
        while let Some(event) = client_events.next().await {
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    println!("🔗 [客户端] 连接建立: {}", session_id);
                    println!("   地址: {} -> {}", info.local_addr, info.peer_addr);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    message_count += 1;
                    println!("📨 [客户端] 收到回显 #{}: {} bytes (会话: {})", 
                        message_count, packet.payload.len(), session_id);
                    
                    if let Some(text) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", text);
                    }
                }
                TransportEvent::MessageSent { session_id, packet_id } => {
                    println!("📤 [客户端] 消息发送成功: {} (包ID: {})", session_id, packet_id);
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    println!("❌ [客户端] 连接关闭: {} (原因: {:?})", session_id, reason);
                    break;
                }
                _ => {
                    println!("ℹ️ [客户端] 其他事件: {:?}", event);
                }
            }
        }
        
        println!("⚠️ 客户端事件流结束");
    });
    
    // 3. 发送测试消息
    sleep(Duration::from_millis(500)).await;
    
    let test_messages = vec![
        "Hello, Event-Driven World!",
        "This is message #2",
        "Testing tokio::select! architecture",
        "Final test message",
    ];
    
    for (i, message) in test_messages.iter().enumerate() {
        let packet = Packet::new(
            PacketType::Data,
            (i + 1) as u32,
            BytesMut::from(message.as_bytes()),
        );
        
        println!("📤 发送消息 #{}: \"{}\"", i + 1, message);
        
        match client.send(packet).await {
            Ok(_) => println!("✅ 消息 #{} 发送成功", i + 1),
            Err(e) => println!("❌ 消息 #{} 发送失败: {:?}", i + 1, e),
        }
        
        // 间隔发送
        sleep(Duration::from_millis(1000)).await;
    }
    
    // 4. 等待处理完成
    println!("⏳ 等待消息处理完成...");
    sleep(Duration::from_secs(3)).await;
    
    // 5. 关闭连接
    println!("🔚 关闭连接...");
    if let Err(e) = client.disconnect().await {
        println!("⚠️ 关闭连接时出错: {:?}", e);
    }
    
    // 等待事件处理完成
    sleep(Duration::from_secs(1)).await;
    
    // 取消任务
    server_task.abort();
    client_task.abort();
    server_serve_task.abort();
    
    println!("✅ 事件驱动TCP通信测试完成");
    
    Ok(())
} 