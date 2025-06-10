/// 修复版事件流调试工具
/// 
/// 解决事件流生命周期问题

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    event::TransportEvent,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志以观察所有事件
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🔍 修复版事件流调试工具");
    println!("======================");
    
    // 只启用TCP服务器来简化测试
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:9999".parse::<std::net::SocketAddr>()?);
        
    let transport = TransportServerBuilder::new()
        .with_protocol(tcp_config)
        .build()
        .await?;
    
    println!("✅ TCP服务器创建完成: 127.0.0.1:9999");
    
    // 🔧 修复：在serve()之前克隆transport以创建事件流
    let transport_for_events = transport.clone();
    let mut events = transport_for_events.events();
    println!("✅ 事件流已创建（使用克隆的transport）");
    
    // 启动事件监听任务
    let event_task = tokio::spawn(async move {
        println!("🎧 开始监听事件...");
        let mut event_count = 0;
        
        while let Some(event) = events.next().await {
            event_count += 1;
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    println!("🔗 [事件 {}] 新连接: {} [{:?}]", event_count, session_id, info.protocol);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    println!("📨 [事件 {}] 收到消息 ({}): {} bytes", event_count, session_id, packet.payload.len());
                    if let Some(text) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", text);
                    }
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    println!("❌ [事件 {}] 连接关闭: {} - {:?}", event_count, session_id, reason);
                }
                TransportEvent::MessageSent { session_id, packet_id } => {
                    println!("📤 [事件 {}] 消息已发送: {} (包ID: {})", event_count, session_id, packet_id);
                }
                _ => {
                    println!("ℹ️ [事件 {}] 其他事件: {:?}", event_count, event);
                }
            }
        }
        
        println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
    });
    
    // 等待一秒让事件监听任务启动
    tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    println!("🚀 事件监听任务已启动，现在启动服务器...");
    
    // 启动服务器（这会消费transport）
    let server_result = tokio::select! {
        result = transport.serve() => {
            println!("🛑 服务器已停止: {:?}", result);
            result
        }
        _ = tokio::signal::ctrl_c() => {
            println!("🛑 收到中断信号，停止服务器");
            Ok(())
        }
    };
    
    // 等待事件任务结束
    let _ = event_task.await;
    
    server_result?;
    Ok(())
} 