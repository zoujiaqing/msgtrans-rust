/// 简化 Echo 服务器 - 基于全新统一API设计
/// 
/// 展示多协议并行服务的正确实现模式

use msgtrans::{
    transport::TransportServerBuilder,
    protocol::{TcpServerConfig, WebSocketServerConfig, QuicServerConfig},
    event::TransportEvent,
    packet::Packet,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 简化 Echo 服务器 - 全新API设计");
    println!("===============================");

    let tcp_server_config = TcpServerConfig::new()
        .with_bind_address("127.0.0.1:8001".parse::<std::net::SocketAddr>()?);
    let web_socket_server_config = WebSocketServerConfig::new()
        .with_bind_address("127.0.0.1:8002".parse::<std::net::SocketAddr>()?);
    let quic_server_config = QuicServerConfig::new()
        .with_bind_address("127.0.0.1:8004".parse::<std::net::SocketAddr>()?);
    
    let transport = TransportServerBuilder::new()
        .max_connections(100)
        .with_protocol(tcp_server_config)
        // .with_protocol(web_socket_server_config)
        .with_protocol(quic_server_config)
        .build()
        .await?;

    println!("✅ 传输实例创建完成");
    println!("📡 TCP 服务器: 127.0.0.1:8001");
    println!("🌐 WebSocket 服务器: 127.0.0.1:8002");
    println!("🔒 QUIC 服务器: 127.0.0.1:8004");
    println!();
    println!("🎯 测试方法:");
    println!("   TCP:       telnet 127.0.0.1 8001");
    println!("   WebSocket: 使用WebSocket客户端连接 ws://127.0.0.1:8002");
    println!("   QUIC:      使用QUIC客户端连接 127.0.0.1:8004");
    println!();
    
    // 获取事件流
    let mut events = transport.events();
    
    // ✅ 克隆transport引用用于echo消息回复
    let transport_for_echo = transport.clone();
    
    // 🚀 启动事件处理任务
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    println!("🔗 新连接: {} [{:?}]", session_id, info.protocol);
                }
                TransportEvent::MessageReceived { session_id, packet } => {
                    println!("📨 收到消息 ({}): {} bytes", session_id, packet.payload.len());
                    if let Some(text) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", text);
                    }
                    
                    // ✅ 实现真正的Echo功能
                    println!("🔄 准备Echo回复到会话 {}", session_id);
                    let echo_packet = Packet::echo(packet.message_id, packet.payload.clone());
                    println!("🔄 创建Echo数据包: ID={}, 大小={}bytes", echo_packet.message_id, echo_packet.payload.len());
                    
                    match transport_for_echo.send_to_session(session_id, echo_packet).await {
                        Ok(()) => {
                            println!("✅ Echo成功: 消息已回复到会话 {}", session_id);
                        }
                        Err(e) => {
                            println!("❌ Echo失败 (会话 {}): {:?}", session_id, e);
                        }
                    }
                }
                TransportEvent::ConnectionClosed { session_id, .. } => {
                    println!("❌ 连接关闭: {}", session_id);
                }
                _ => {}
            }
        }
    });

    // 🚀 启动服务器 - 这是主要的服务监听循环
    transport.serve().await?;
    
    Ok(())
}
