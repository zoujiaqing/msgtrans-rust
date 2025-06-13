/// QUIC Echo 服务器测试
/// 
/// 测试QUIC适配器的事件循环是否正常工作

use msgtrans::{
    transport::{TransportServerBuilder, TransportClientBuilder}, 
    Packet, PacketType,
    protocol::{QuicServerConfig, QuicClientConfig},
};
use std::time::Duration;
use tokio::time::sleep;
use futures_util::StreamExt;
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🚀 启动QUIC Echo服务器测试");
    
    // 1. 创建QUIC服务器配置
    let quic_server_config = QuicServerConfig {
        bind_address: "127.0.0.1:12345".parse().unwrap(),
        ..Default::default()
    };
    
    // 创建服务器
    let server = TransportServerBuilder::new()
        .with_protocol(quic_server_config)
        .build()
        .await?;
    
    // 获取服务器事件流
    let mut server_events = server.events();
    
    // 启动服务器
    let server_task = {
        let server = server.clone();
        tokio::spawn(async move {
            if let Err(e) = server.serve().await {
                eprintln!("❌ 服务器错误: {:?}", e);
            }
        })
    };
    
    // 等待服务器启动
    sleep(Duration::from_millis(500)).await;
    
    // 2. 创建客户端连接
    let quic_client_config = QuicClientConfig {
        target_address: "127.0.0.1:12345".parse().unwrap(),
        ..Default::default()
    };
    
    let mut client = TransportClientBuilder::new()
        .with_protocol(quic_client_config)
        .build()
        .await?;
    
    // 连接到服务器
    client.connect().await?;
    let mut client_events = client.events().await?;
    
    // 3. 启动服务器事件处理
    let echo_task = tokio::spawn(async move {
        println!("📡 开始监听服务器事件...");
        
        while let Some(event) = server_events.next().await {
            println!("📥 服务器事件: {:?}", event);
            
            match event {
                msgtrans::TransportEvent::ConnectionEstablished { session_id, .. } => {
                    println!("🔗 新连接建立: {}", session_id);
                }
                msgtrans::TransportEvent::MessageReceived { session_id, packet } => {
                    println!("📨 收到消息 (会话 {}): {:?}", session_id, String::from_utf8_lossy(&packet.payload));
                    
                    // Echo回消息
                    let echo_payload = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                    let echo_packet = Packet {
                        packet_type: PacketType::Data,
                        message_id: packet.message_id,
                        payload: BytesMut::from(echo_payload.as_bytes()),
                    };
                    
                    if let Err(e) = server.send_to_session(session_id, echo_packet).await {
                        eprintln!("❌ 发送echo失败: {:?}", e);
                    } else {
                        println!("✅ Echo消息已发送");
                    }
                }
                msgtrans::TransportEvent::ConnectionClosed { session_id, .. } => {
                    println!("🔌 连接关闭: {}", session_id);
                    break;
                }
                _ => {}
            }
        }
        
        println!("📡 服务器事件监听结束");
    });
    
    // 4. 客户端发送测试消息
    let client_task = tokio::spawn(async move {
        sleep(Duration::from_millis(200)).await;
        
        println!("📤 客户端发送测试消息...");
        
        let test_messages = vec![
            "Hello, QUIC Server!",
            "测试中文消息",
            "Message with numbers: 12345",
        ];
        
        for (i, msg) in test_messages.iter().enumerate() {
            let packet = Packet {
                packet_type: PacketType::Data,
                message_id: (i + 1) as u32,
                payload: BytesMut::from(msg.as_bytes()),
            };
            
            if let Err(e) = client.send(packet).await {
                eprintln!("❌ 发送消息失败: {:?}", e);
            } else {
                println!("✅ 消息已发送: {}", msg);
            }
            
            sleep(Duration::from_millis(500)).await;
        }
        
        // 监听客户端事件
        println!("📡 监听客户端事件...");
        let mut event_count = 0;
        while let Ok(event_result) = tokio::time::timeout(Duration::from_secs(2), client_events.next()).await {
            match event_result {
                Ok(event) => {
                    println!("📥 客户端事件: {:?}", event);
                    event_count += 1;
                    
                    if event_count >= 3 {
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("❌ 客户端事件错误: {:?}", e);
                    break;
                }
            }
        }
        
        println!("📡 客户端事件监听结束");
    });
    
    // 等待任务完成
    tokio::select! {
        _ = client_task => println!("✅ 客户端任务完成"),
        _ = echo_task => println!("✅ Echo任务完成"),
        _ = server_task => println!("✅ 服务器任务完成"),
        _ = sleep(Duration::from_secs(10)) => println!("⏰ 测试超时"),
    }
    
    println!("🎉 QUIC Echo服务器测试完成");
    Ok(())
} 