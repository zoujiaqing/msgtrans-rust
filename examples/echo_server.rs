 /// Echo服务器 - 展示统一API设计（方案A）
use msgtrans::{
    transport::TransportBuilder,
    protocol::{TcpConfig, WebSocketConfig, QuicConfig},
    Event, Packet,
};
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 统一API Echo服务器（方案A演示）");
    println!("===============================");
    
    // 🎯 统一配置：所有协议在Builder阶段配置
    let transport = TransportBuilder::new()
        .with_protocol_config(TcpConfig {
            bind_address: "127.0.0.1:8001".parse()?,
            ..Default::default()
        })
        .with_protocol_config(WebSocketConfig {
            bind_address: "127.0.0.1:8002".parse()?,
            ..Default::default()
        })
        .with_protocol_config(QuicConfig {
            bind_address: "127.0.0.1:8003".parse()?,
            ..Default::default()
        })
        .high_performance() // 专家配置
        .build()
        .await?;
    
    println!("✅ 传输实例创建完成");
    println!("📡 TCP 服务器: 127.0.0.1:8001");
    println!("🌐 WebSocket 服务器: 127.0.0.1:8002");
    println!("🔒 QUIC 服务器: 127.0.0.1:8003");
    println!();
    println!("🎯 测试方法:");
    println!("   TCP:       telnet 127.0.0.1 8001");
    println!("   WebSocket: 使用WebSocket客户端连接 ws://127.0.0.1:8002");
    println!("   QUIC:      使用QUIC客户端连接 127.0.0.1:8003");
    println!();
    
    // 启动事件处理
    let mut events = transport.events();
    let event_transport = transport.clone();
    
    tokio::spawn(async move {
        while let Some(event) = events.next().await {
            match event {
                Event::ConnectionEstablished { session_id, info } => {
                    println!("🔗 新连接: {} [{:?}]", session_id, info.protocol);
                }
                Event::MessageReceived { session_id, packet } => {
                    println!("📨 收到消息 ({}): {} bytes", session_id, packet.payload.len());
                    if let Some(text) = packet.payload_as_string() {
                        println!("   内容: \"{}\"", text);
                    }
                    
                    // Echo: 发送相同的消息回去，但使用Echo类型
                    let echo_packet = Packet::echo(packet.message_id, packet.payload.clone());
                    if event_transport.send_to_session(session_id, echo_packet).await.is_ok() {
                        println!("✅ 回显成功");
                    }
                }
                Event::ConnectionClosed { session_id, .. } => {
                    println!("❌ 连接关闭: {}", session_id);
                }
                _ => {}
            }
        }
    });

    // 🚀 统一启动：一个方法启动所有服务器
    transport.serve().await?;
    
    Ok(())
}
