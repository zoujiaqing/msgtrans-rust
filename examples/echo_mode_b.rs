/// Echo服务器 - 模式B：先启动服务器，后处理事件
/// 🎯 验证"顺序无关"API设计
/// 
/// 这种模式使用spawn_server并发启动

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    event::TransportEvent,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🎯 Echo服务器 - 模式B (先启动后处理)");
    println!("===================================");
    
    // 创建传输层
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:9998".parse::<std::net::SocketAddr>()?);
        
    let transport = TransportServerBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
    
    println!("✅ Transport 创建完成");
    
    // 🎯 关键：先启动服务器到后台
    let transport_clone = transport.clone();
    let _server_handle = transport_clone.spawn_server().await?;
    println!("🚀 服务器已在后台启动");
    
    // 🎯 然后创建事件流并处理
    let mut events = transport.events();
    println!("📡 事件流创建完成 - 服务器已在运行");
    
    println!("🎧 开始监听事件...");
    println!("📝 使用不同端口测试: 'telnet 127.0.0.1 9998'");
    
    let mut event_count = 0u64;
    
    // 事件处理循环
    while let Some(event) = events.next().await {
        event_count += 1;
        println!("📥 事件 #{}: {:?}", event_count, event);
        
        match event {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                println!("🔗 新连接建立: {} <- {}", session_id, info.peer_addr);
            }
            TransportEvent::MessageReceived { session_id, packet } => {
                println!("📨 收到消息 (会话: {}): {:?}", session_id, packet);
                
                // 发送回显
                let echo_message = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                let echo_packet = msgtrans::Packet::data(packet.message_id + 1000, echo_message.as_bytes());
                
                match transport.send_to_session(session_id, echo_packet).await {
                    Ok(()) => {
                        println!("✅ 回显消息已发送到会话: {}", session_id);
                    }
                    Err(e) => {
                        println!("❌ 发送回显失败: {:?}", e);
                    }
                }
            }
            TransportEvent::MessageSent { session_id, packet_id } => {
                println!("📤 消息已发送 (会话: {}, 消息ID: {})", session_id, packet_id);
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
        
        // 演示：处理一些事件后可以选择退出
        if event_count >= 100 {
            println!("🏁 已处理 {} 个事件，准备退出", event_count);
            break;
        }
    }
    
    println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
    
    Ok(())
} 