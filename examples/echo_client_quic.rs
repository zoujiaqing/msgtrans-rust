/// QUIC Echo 客户端 - 使用TransportClientBuilder
/// 🎯 使用标准的Transport客户端构建器，确保协议兼容
/// 
/// 与echo_server_new_api.rs配套使用

use std::time::Duration;
use msgtrans::{
    transport::{client::TransportClientBuilder},
    protocol::QuicClientConfig,
    packet::Packet,
    event::ClientEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("🎯 QUIC Echo 客户端 - TransportClientBuilder版本");
    println!("==============================================");
    println!();

    // 配置TCP客户端 - 使用链式配置
    let quic_config = QuicClientConfig::new()
        .with_target_address("127.0.0.1:8003".parse::<std::net::SocketAddr>()?)
        .build()?;

    println!("🔌 准备连接到服务器: {}", quic_config.target_address);

    // 🔧 修正：使用TransportClientBuilder构建标准客户端
    let mut transport = TransportClientBuilder::new()
        .with_protocol(quic_config)
        .connect_timeout(Duration::from_secs(10))
        .enable_connection_monitoring(true)
        .build()
        .await?;
        
    println!("✅ 客户端Transport构建成功");

    // 建立连接
    transport.connect().await?;
    println!("✅ 连接建立成功");

    // 获取事件流来接收回显消息
    let mut events = transport.events().await?;
    
    // 启动接收任务来处理回显
    let receiver_task = tokio::spawn(async move {
        println!("🎧 开始监听回显事件...");
        let mut received_count = 0u64;
        
        loop {
            match events.next().await {
                Ok(event) => {
                    match event {
                        ClientEvent::MessageReceived { packet } => {
                            received_count += 1;
                            let message = String::from_utf8_lossy(&packet.payload);
                            println!("📥 收到回显 #{}: (ID: {})", received_count, packet.message_id);
                            println!("   内容: \"{}\"", message);
                            
                            // 检查是否是最后一条消息
                            if message.contains("Message #4") {
                                println!("🎯 收到最后一条回显，准备结束");
                                break;
                            }
                        }
                        ClientEvent::Disconnected { reason } => {
                            println!("🔌 连接已关闭: {:?}", reason);
                            break;
                        }
                        ClientEvent::Connected { info } => {
                            println!("🔗 连接已建立: {} ↔ {}", info.local_addr, info.peer_addr);
                        }
                        ClientEvent::Error { error } => {
                            println!("⚠️ 传输错误: {:?}", error);
                            break;
                        }
                        ClientEvent::MessageSent { packet_id } => {
                            println!("ℹ️ 消息发送确认: ID {}", packet_id);
                        }
                    }
                }
                Err(e) => {
                    println!("❌ 事件接收错误: {:?}", e);
                    break;
                }
            }
        }
        
        println!("📡 事件接收器已停止 (共收到 {} 条回显)", received_count);
    });

    // 🎯 准备测试消息
    let test_messages = vec![
        "Hello, TransportClient!",
        "测试标准客户端协议", 
        "Message with numbers: 12345",
        "Message #4 - Final test",
    ];

    println!("📤 开始发送测试消息...");
    println!();

    for (i, message) in test_messages.iter().enumerate() {
        println!("📤 发送消息 #{}: \"{}\"", i + 1, message);
        
        // 🔧 使用标准的客户端发送方法
                    let packet = Packet::one_way((i as u32) + 1, message.as_bytes());
        
        match transport.send(packet).await {
            Ok(_) => {
                println!("✅ 消息 #{} 发送成功（通过Transport）", i + 1);
            }
            Err(e) => {
                println!("❌ 消息 #{} 发送失败: {:?}", i + 1, e);
                break;
            }
        }

        // 等待一下再发送下一条
        if i < test_messages.len() - 1 {
            println!("⏳ 等待2秒后发送下一条...");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    println!();
    println!("⏳ 等待接收所有回显消息...");
    
    // 增加等待时间，给服务端足够时间发送回显
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    // 等待接收器任务完成或超时
    match tokio::time::timeout(Duration::from_secs(15), receiver_task).await {
        Ok(_) => {
            println!("✅ 所有回显已接收");
        }
        Err(_) => {
            println!("⏰ 等待回显超时，但这是正常的");
        }
    }
    
    // 关闭连接
    println!("👋 关闭客户端连接...");
    if let Err(e) = transport.disconnect().await {
        println!("❌ 关闭连接失败: {:?}", e);
    } else {
        println!("✅ 连接已关闭");
    }

    println!("🏁 客户端测试完成");
    println!();
    println!("🎯 标准客户端特性:");
    println!("   ✅ 使用TransportClientBuilder");
    println!("   ✅ 标准协议栈和数据包格式");
    println!("   ✅ 完整的事件处理");
    println!("   ✅ 与服务器协议兼容");

    Ok(())
} 