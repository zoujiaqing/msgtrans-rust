/// Echo服务器 - 新API设计演示
/// 🎯 使用推荐的模式A：先定义事件处理，后启动服务器
/// 
/// 专注于TCP协议，展示事件流的完整功能

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::TcpServerConfig,
    protocol::WebSocketServerConfig,
    protocol::QuicServerConfig,
    event::{ServerEvent, RequestContext},
    packet::Packet,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志以观察事件流
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🎯 Echo服务器 - 新API设计演示");
    println!("=============================");
    println!("📋 使用模式A：先定义事件处理，后启动服务器");
    println!();
    
    // 创建TCP服务器配置
    let tcp_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:8001".parse::<std::net::SocketAddr>()?);
    let web_socket_server_config = WebSocketServerConfig::new()
        .with_bind_address("127.0.0.1:8002".parse::<std::net::SocketAddr>()?);
    let quic_server_config = QuicServerConfig::new()
        .with_bind_address("127.0.0.1:8003".parse::<std::net::SocketAddr>()?);
    
    let transport = TransportServerBuilder::new()
        .max_connections(10)  // 限制连接数便于测试
        .with_protocol(tcp_config)
        .with_protocol(web_socket_server_config)
        .with_protocol(quic_server_config)
        .build()
        .await?;
    
    println!("✅ TCP服务器创建完成: 127.0.0.1:8001");
    
    // 🎯 核心：立即创建事件流（此时服务器还未启动）
    let mut events = transport.subscribe_events();
    println!("📡 事件流创建完成 - 服务器尚未启动");
    
    // 克隆transport用于在事件处理中发送回显
    let transport_for_echo = transport.clone();
    
    // 🎯 模式A：先定义完整的事件处理逻辑
    let event_task = tokio::spawn(async move {
        println!("🎧 开始监听事件...");
        let mut event_count = 0u64;
        let mut connections = std::collections::HashSet::new();
        
        while let Ok(event) = events.recv().await {
            event_count += 1;
            println!("📥 事件 #{}: {:?}", event_count, event);
            
            match event {
                ServerEvent::ConnectionEstablished { session_id, info } => {
                    connections.insert(session_id);
                    println!("🔗 新连接建立: {} <- {} (协议: {:?})", session_id, info.peer_addr, info.protocol);
                    println!("   当前连接数: {}", connections.len());
                }
                ServerEvent::ConnectionClosed { session_id, reason } => {
                    connections.remove(&session_id);
                    println!("🔌 连接关闭: {} (原因: {:?})", session_id, reason);
                    println!("   剩余连接数: {}", connections.len());
                }
                ServerEvent::MessageReceived { session_id, packet } => {
                    let message_text = String::from_utf8_lossy(&packet.payload);
                    println!("📩 收到消息: 会话: {}, ID: {}, 内容: {}", session_id, packet.message_id, message_text);
                }
                ServerEvent::MessageSent { session_id, packet_id } => {
                    println!("📤 消息发送确认: 会话 {}, 消息ID {}", session_id, packet_id);
                }
                ServerEvent::TransportError { session_id, error } => {
                    println!("⚠️ 传输错误: {:?} (会话: {:?})", error, session_id);
                }
                ServerEvent::ServerStarted { address } => {
                    println!("🌟 服务器启动通知: {}", address);
                }
                ServerEvent::ServerStopped => {
                    println!("🛑 服务器停止通知");
                }
                ServerEvent::RequestReceived { session_id, ctx } => {
                    println!("🔄 收到请求: 会话: {}, ID: {}", session_id, ctx.request.message_id);
                    ctx.respond_with(|req| {
                        let mut resp = req.clone();
                        resp.payload = format!("Echo: {}", String::from_utf8_lossy(&req.payload)).into_bytes();
                        resp
                    });
                }
            }
        }
        
        println!("⚠️ 事件流已结束 (共处理 {} 个事件)", event_count);
        println!("   最终连接数: {}", connections.len());
    });
    
    println!("🚀 事件处理任务已启动");
    println!("🌐 现在启动服务器...");
    println!();
    println!("🎯 测试方法:");
    println!("   在另一个终端运行: cargo run --example echo_client_tcp_new");
    println!("   或使用: telnet 127.0.0.1 8001");
    println!();
    
    // 🎯 模式A的关键：现在才启动服务器，但事件流已经在监听了
    let server_result = transport.serve().await;
    
    println!("🏁 服务器已停止");
    
    // 等待事件处理完成
    let _ = event_task.await;
    
    if let Err(e) = server_result {
        println!("💥 服务器错误: {:?}", e);
    }
    
    Ok(())
} 