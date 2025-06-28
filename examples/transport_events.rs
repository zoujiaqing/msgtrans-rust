/// 调试传输事件流的专用程序
/// 
/// 用于诊断无锁连接的事件桥接和服务端事件处理

use std::time::Duration;
use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    event::ServerEvent,
    transport::TransportClientBuilder,
    protocol::TcpClientConfig,
    event::ClientEvent,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用最详细的日志以观察所有事件流
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)  // 🔍 最详细的日志级别
        .init();
    
    println!("🔍 传输事件流调试程序");
    println!("====================");
    println!("观察无锁连接的事件桥接和服务端事件处理");
    println!();
    
    // 启动服务端 - 简化API
    let tcp_config = TcpServerConfig::new("127.0.0.1:9001")?;
        
    let server = TransportServerBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
    
    println!("✅ 服务端创建完成: 127.0.0.1:9001");
    
    // 订阅服务端事件
    let mut server_events = server.subscribe_events();
    let server_clone = server.clone();
    
    // 服务端事件处理
    let server_task = tokio::spawn(async move {
        println!("🎧 服务端事件处理启动");
        let mut event_count = 0;
        
        while let Ok(event) = server_events.recv().await {
            event_count += 1;
            println!("📥 服务端事件 #{}: {:?}", event_count, event);
            
            match event {
                ServerEvent::ConnectionEstablished { session_id, .. } => {
                    println!("🎉 新连接建立: {}", session_id);
                    
                    // 🚀 修复：将发送操作移到单独的异步任务中，避免阻塞事件循环
                    let server_for_send = server_clone.clone();
                    tokio::spawn(async move {
                        // 立即发送一条消息测试
                        if let Err(e) = server_for_send.send(session_id, b"Hello from server!").await {
                            println!("❌ 服务端发送失败: {:?}", e);
                        } else {
                            println!("✅ 服务端发送成功");
                        }
                    });
                }
                ServerEvent::MessageReceived { session_id, context } => {
                    println!("📨 收到消息 (会话: {}, ID: {}, 请求: {}): {}", 
                        session_id, 
                        context.message_id, 
                        context.is_request(),
                        context.as_text_lossy()
                    );
                    
                    if context.is_request() {
                        let response = format!("Echo: {}", context.as_text_lossy());
                        println!("📤 响应请求...");
                        context.respond(response.into_bytes());
                        println!("✅ 请求已响应");
                    }
                }
                ServerEvent::MessageSent { session_id, message_id } => {
                    println!("📤 消息发送确认: 会话 {}, ID {}", session_id, message_id);
                }
                ServerEvent::ConnectionClosed { session_id, reason } => {
                    println!("🔌 连接关闭: 会话 {}, 原因: {:?}", session_id, reason);
                    break;
                }
                _ => {
                    println!("📝 其他事件: {:?}", event);
                }
            }
        }
        
        println!("⚠️ 服务端事件处理结束 (处理了 {} 个事件)", event_count);
    });
    
    // 启动服务端
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.serve().await {
            println!("❌ 服务端错误: {:?}", e);
        }
    });
    
    // 等待服务端启动
    tokio::time::sleep(Duration::from_millis(500)).await;
    println!("🚀 服务端已启动，现在启动客户端");
    
    // 启动客户端 - 简化API
    let client_config = TcpClientConfig::new("127.0.0.1:9001")?;
    let mut client = TransportClientBuilder::new()
        .with_protocol(client_config)
        .build()
        .await?;
    
    let mut client_events = client.subscribe_events();
    
    // 客户端事件处理
    let client_task = tokio::spawn(async move {
        println!("🎧 客户端事件处理启动");
        let mut event_count = 0;
        
        while let Ok(event) = client_events.recv().await {
            event_count += 1;
            println!("📥 客户端事件 #{}: {:?}", event_count, event);
            
            match event {
                ClientEvent::Connected { .. } => {
                    println!("🎉 客户端连接成功");
                }
                ClientEvent::MessageReceived(context) => {
                    println!("📨 客户端收到消息 (ID: {}): {}", 
                        context.message_id, 
                        context.as_text_lossy()
                    );
                    
                    if context.is_request() {
                        println!("📤 响应服务端请求...");
                        context.respond(b"Client response!".to_vec());
                        println!("✅ 已响应服务端请求");
                    }
                }
                ClientEvent::MessageSent { message_id } => {
                    println!("📤 客户端消息发送确认: ID {}", message_id);
                }
                ClientEvent::Disconnected { .. } => {
                    println!("🔌 客户端连接断开");
                    break;
                }
                _ => {
                    println!("📝 客户端其他事件: {:?}", event);
                }
            }
        }
        
        println!("⚠️ 客户端事件处理结束 (处理了 {} 个事件)", event_count);
    });
    
    // 连接到服务端
    println!("🔌 客户端正在连接...");
    client.connect().await?;
    println!("✅ 客户端连接成功");
    
    // 等待连接稳定
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 发送单向消息
    println!("📤 发送单向消息...");
    let result = client.send(b"Hello Server!").await?;
    println!("✅ 单向消息发送成功 (ID: {})", result.message_id);
    
    // 等待一下
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 发送请求
    println!("🔄 发送请求...");
    let response = client.request(b"What time is it?").await?;
    if let Some(data) = response.data {
        println!("✅ 收到响应: {}", String::from_utf8_lossy(&data));
    } else {
        println!("⚠️ 请求超时或无响应");
    }
    
    // 等待事件处理
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    // 断开连接
    println!("🔌 断开连接...");
    client.disconnect().await?;
    
    // 等待任务完成
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // 清理
    server_handle.abort();
    let _ = tokio::join!(server_task, client_task);
    
    println!("🏁 调试程序完成");
    Ok(())
} 