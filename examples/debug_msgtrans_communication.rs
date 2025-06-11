/// 详细的msgtrans通信调试测试
/// 分别测试发送和接收，确认问题所在

use std::time::Duration;
use msgtrans::{
    transport::{client::TransportClientBuilder, server::TransportServerBuilder},
    protocol::{TcpClientConfig, TcpServerConfig},
    packet::Packet,
    event::TransportEvent,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("🔧 详细的msgtrans通信调试测试");
    println!("==============================");

    // 配置服务端
    let server_config = TcpServerConfig {
        bind_address: "127.0.0.1:8004".parse()?,
        max_connections: 100,
        nodelay: true,
        keepalive: Some(Duration::from_secs(60)),
        read_buffer_size: 8192,
        write_buffer_size: 8192,
        accept_timeout: Duration::from_secs(30),
        idle_timeout: Some(Duration::from_secs(300)),
        reuse_port: false,
        reuse_addr: true,
    };

    // 启动服务端
    let server_task = tokio::spawn(async move {
        println!("🚀 启动服务端...");
        
        let mut server = TransportServerBuilder::new()
            .with_protocol("tcp", server_config)
            .build()
            .await
            .unwrap();

        let mut events = server.events();
        
        // 启动服务端事件处理
        let event_task = tokio::spawn(async move {
            println!("🎧 服务端开始监听事件...");
            
            while let Some(event) = events.next().await {
                match event {
                    TransportEvent::ConnectionEstablished { session_id, info } => {
                        println!("🔗 服务端：新连接建立 {} <- {}", session_id, info.peer_addr);
                    }
                    TransportEvent::MessageReceived { session_id, packet } => {
                        let message = String::from_utf8_lossy(&packet.payload);
                        println!("📥 服务端收到消息: \"{}\" (会话: {}, ID: {})", message, session_id, packet.message_id);
                        
                        // 立即发送回显
                        let echo_message = format!("Echo: {}", message);
                        let echo_packet = Packet::data(packet.message_id + 1000, echo_message.as_bytes());
                        
                        println!("📤 服务端发送回显: \"{}\"", echo_message);
                        // 这里需要通过server发送，但目前的API设计可能有问题
                        // 暂时先打印日志
                    }
                    TransportEvent::ConnectionClosed { session_id, reason } => {
                        println!("🔌 服务端：连接关闭 {} - {:?}", session_id, reason);
                    }
                    _ => {
                        println!("ℹ️ 服务端其他事件: {:?}", event);
                    }
                }
            }
        });

        // 启动服务端监听
        println!("🌟 服务端开始监听...");
        if let Err(e) = server.serve().await {
            println!("❌ 服务端错误: {:?}", e);
        }
        
        event_task.abort();
        println!("🔚 服务端结束");
    });

    // 等待服务端启动
    tokio::time::sleep(Duration::from_secs(1)).await;

    // 配置客户端
    let client_config = TcpClientConfig {
        target_address: "127.0.0.1:8004".parse()?,
        connect_timeout: Duration::from_secs(10),
        read_timeout: Some(Duration::from_secs(30)),
        write_timeout: Some(Duration::from_secs(10)),
        nodelay: true,
        keepalive: Some(Duration::from_secs(60)),
        read_buffer_size: 8192,
        write_buffer_size: 8192,
        retry_config: Default::default(),
        local_bind_address: None,
    };

    // 启动客户端
    let client_task = tokio::spawn(async move {
        println!("🔌 启动客户端...");
        
        let mut client = TransportClientBuilder::new()
            .with_protocol(client_config)
            .connect_timeout(Duration::from_secs(10))
            .build()
            .await
            .unwrap();

        // 建立连接
        println!("🔗 客户端连接中...");
        client.connect().await.unwrap();
        println!("✅ 客户端连接成功");

        // 获取事件流
        let mut events = client.events().await.unwrap();
        
        // 启动客户端事件处理
        let event_task = tokio::spawn(async move {
            println!("🎧 客户端开始监听事件...");
            let mut received_count = 0u64;
            
            while let Some(event) = events.next().await {
                match event {
                    TransportEvent::ConnectionEstablished { session_id, .. } => {
                        println!("🔗 客户端：连接已建立 (会话: {})", session_id);
                    }
                    TransportEvent::MessageReceived { session_id, packet } => {
                        received_count += 1;
                        let message = String::from_utf8_lossy(&packet.payload);
                        println!("📥 客户端收到回显 #{}: \"{}\" (会话: {}, ID: {})", 
                            received_count, message, session_id, packet.message_id);
                        
                        if received_count >= 2 {
                            println!("🎯 客户端收到足够回显，准备结束");
                            break;
                        }
                    }
                    TransportEvent::ConnectionClosed { session_id, reason } => {
                        println!("🔌 客户端：连接已关闭 (会话: {}): {:?}", session_id, reason);
                        break;
                    }
                    _ => {
                        println!("ℹ️ 客户端其他事件: {:?}", event);
                    }
                }
            }
            
            println!("📡 客户端事件处理结束 (共收到 {} 条回显)", received_count);
        });

        // 发送测试消息
        println!("📤 客户端开始发送测试消息...");
        
        let test_messages = vec![
            "Hello from debug client!",
            "Second test message",
        ];

        for (i, message) in test_messages.iter().enumerate() {
            println!("📤 发送消息 #{}: \"{}\"", i + 1, message);
            
            let packet = Packet::data((i as u32) + 1, message.as_bytes());
            
            match client.send(packet).await {
                Ok(_) => {
                    println!("✅ 消息 #{} 发送成功", i + 1);
                }
                Err(e) => {
                    println!("❌ 消息 #{} 发送失败: {:?}", i + 1, e);
                    break;
                }
            }

            // 等待一下再发送下一条
            tokio::time::sleep(Duration::from_secs(1)).await;
        }

        println!("⏳ 等待接收回显...");
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        event_task.abort();
        
        // 关闭连接
        if let Err(e) = client.disconnect().await {
            println!("❌ 关闭连接失败: {:?}", e);
        } else {
            println!("✅ 客户端连接已关闭");
        }
        
        println!("🔚 客户端结束");
    });

    // 等待两个任务完成
    let (server_result, client_result) = tokio::join!(server_task, client_task);
    
    match (server_result, client_result) {
        (Ok(()), Ok(())) => {
            println!("🎉 详细调试测试完成！");
        }
        (server_err, client_err) => {
            println!("❌ 测试失败:");
            if let Err(e) = server_err {
                println!("  服务端错误: {:?}", e);
            }
            if let Err(e) = client_err {
                println!("  客户端错误: {:?}", e);
            }
        }
    }

    Ok(())
} 