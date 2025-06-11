/// Echo服务器 - 模式A：先定义事件处理，后启动服务器
/// 🎯 验证"顺序无关"API设计
/// 
/// 这种模式更符合"配置->运行"的思维模式

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
    
    println!("🎯 Echo服务器 - 模式A (先处理后启动)");
    println!("===================================");
    
    // 创建传输层
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:9999".parse::<std::net::SocketAddr>()?);
        
    let transport = TransportServerBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
    
    println!("✅ Transport 创建完成");
    
    // 🎯 关键：立即创建事件流，此时服务器还未启动
    let mut events = transport.events();
    println!("📡 事件流创建完成 - 服务器尚未启动");
    
    // 🔄 克隆transport用于事件处理
    let transport_for_events = transport.clone();
    
    // 启动事件处理任务
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
                    
                    // 发送回显
                    let echo_message = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                    let echo_packet = msgtrans::Packet::data(packet.message_id + 1000, echo_message.as_bytes());
                    
                    match transport_for_events.send_to_session(session_id, echo_packet).await {
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
        }
        
        println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
    });
    
    println!("🚀 事件处理任务已启动");
    println!("🌐 现在启动服务器...");
    
    // 🎯 关键：现在才启动服务器，但事件流已经在监听了
    // 这证明了事件流不依赖服务器启动状态
    let server_result = transport.serve().await;
    
    println!("🏁 服务器已停止");
    
    // 等待事件处理完成
    let _ = event_task.await;
    
    if let Err(e) = server_result {
        println!("💥 服务器错误: {:?}", e);
    }
    
    Ok(())
} 