/// Echo服务器 - 修复版
/// 
/// 完整的回显功能：接收消息并发送回给客户端

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    event::TransportEvent,  // 🔧 现在直接使用TransportEvent
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志以观察所有事件
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🎯 Echo服务器 - 修复版");
    println!("=================");
    
    // 只启用TCP服务器来简化测试
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:9999".parse::<std::net::SocketAddr>()?);
        
    let transport = TransportServerBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
    
    println!("✅ TCP服务器创建完成: 127.0.0.1:9999");
    
    // 创建事件流
    let mut events = transport.events();
    
    // 启动事件处理任务
    let transport_clone = transport.clone();
    let event_task = tokio::spawn(async move {
        println!("🎧 开始监听事件...");
        let mut event_count = 0u64;
        
        while let Some(event) = events.next().await {
            event_count += 1;
            println!("📥 事件 #{}: {:?}", event_count, event);
            
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    println!("🔗 新连接建立: {} <- {}", session_id, info.peer_addr);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    println!("📨 收到消息 (会话: {}): {:?}", session_id, packet);
                    
                    // 🔧 核心修复：收到消息后发送回显
                    let echo_message = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                    let echo_packet = msgtrans::Packet::data(packet.message_id + 1000, echo_message.as_bytes());
                    
                    match transport_clone.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("✅ 回显消息已发送到会话: {}", session_id);
                        }
                        Err(e) => {
                            println!("❌ 发送回显失败: {:?}", e);
                        }
                    }
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    println!("🔌 连接关闭: {} (原因: {:?})", session_id, reason);
                }
                TransportEvent::TransportError { session_id, error } => {
                    println!("⚠️ 传输错误: {:?} (会话: {:?})", error, session_id);
                }
                _ => {
                    println!("ℹ️ 其他事件: {:?}", event);
                }
            }
        }
        
        println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
    });
    
    println!("🚀 事件监听任务已启动，现在启动服务器...");
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    // 启动服务器
    println!("🌐 服务器启动中...");
    
    // 在另一个任务中运行服务器
    let server_task = tokio::spawn(async move {
        if let Err(e) = transport.serve().await {
            println!("💥 服务器错误: {:?}", e);
        }
    });
    
    println!("✅ 服务器已在后台启动");
    println!("🔍 等待客户端连接...");
    println!("📝 使用 'cargo run --example debug_client' 进行测试");
    
    // 等待事件处理完成或用户中断
    tokio::select! {
        _ = event_task => {
            println!("🏁 事件处理任务完成");
        }
        _ = server_task => {
            println!("🏁 服务器任务完成");
        }
        _ = tokio::signal::ctrl_c() => {
            println!("👋 收到中断信号，正在关闭...");
        }
    }
    
    Ok(())
} 