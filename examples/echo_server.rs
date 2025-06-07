/// Echo服务器 - 支持TCP、WebSocket、QUIC多协议回显
use msgtrans::{
    Builder, Config, Event, Packet,
    protocol::adapter::{TcpConfig, WebSocketConfig, QuicConfig},
};
use futures::StreamExt;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    
    println!("🌟 msgtrans 多协议Echo服务器");
    println!("==========================");
    
    let config = Config::default();
    let transport = Builder::new().config(config).build().await?;
    
    // 启动TCP服务器
    let tcp_config = TcpConfig::new("127.0.0.1:8001")?.with_nodelay(true);
    let _tcp_server = transport.listen(tcp_config).await?;
    println!("✅ TCP服务器启动: 127.0.0.1:8001");
    
    // 启动WebSocket服务器
    let ws_config = WebSocketConfig::new("127.0.0.1:8002")?
        .with_path("/echo")
        .with_max_frame_size(64 * 1024);
    let _ws_server = transport.listen(ws_config).await?;
    println!("✅ WebSocket服务器启动: 127.0.0.1:8002");
    
    // 启动QUIC服务器
    let quic_config = QuicConfig::new("127.0.0.1:8003")?
        .with_max_idle_timeout(Duration::from_secs(60))
        .with_keep_alive_interval(Some(Duration::from_secs(30)))
        .with_max_concurrent_streams(100);
    let _quic_server = transport.listen(quic_config).await?;
    println!("✅ QUIC服务器启动: 127.0.0.1:8003");
    
    println!("\n🎯 测试方法:");
    println!("   TCP:       cargo run --example echo_client_tcp");
    println!("   WebSocket: cargo run --example echo_client_websocket");
    println!("   QUIC:      cargo run --example echo_client_quic");
    println!("   Telnet:    telnet 127.0.0.1 8001");
    println!();
    
    let mut events = transport.events();
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
                
                let echo_packet = Packet::data(packet.message_id, packet.payload.clone());
                if transport.send_to_session(session_id, echo_packet).await.is_ok() {
                    println!("✅ 回显成功");
                }
            }
            Event::ConnectionClosed { session_id, .. } => {
                println!("❌ 连接关闭: {}", session_id);
            }
            _ => {}
        }
    }
    Ok(())
}