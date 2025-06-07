/// WebSocket Echo客户端 - 连接到Echo服务器进行测试
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    Builder, Config, Event, Packet,
    protocol::adapter::WebSocketConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    
    println!("🌟 msgtrans WebSocket Echo客户端");
    println!("=============================");
    
    let config = Config::default();
    let transport = Builder::new().config(config).build().await?;
    
    // 连接到服务器
    println!("🔌 连接到WebSocket Echo服务器: 127.0.0.1:8002");
    let ws_config = WebSocketConfig::new("127.0.0.1:8002")?
        .with_path("/echo")
        .with_max_frame_size(64 * 1024);
    
    match transport.connect(ws_config).await {
        Ok(session_id) => {
            println!("✅ WebSocket连接建立成功 (SessionId: {})", session_id);
            
            // 启动事件监听
            let mut events = transport.events();
            let transport_clone = transport.clone();
            
            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    match event {
                        Event::MessageReceived { session_id, packet } => {
                            println!("📨 收到WebSocket回显 (会话{}):", session_id);
                            if let Some(content) = packet.payload_as_string() {
                                println!("   内容: \"{}\"", content);
                            }
                            println!("   ✅ WebSocket回显接收成功");
                        }
                        Event::ConnectionClosed { session_id, reason } => {
                            println!("❌ WebSocket连接关闭: 会话{}, 原因: {:?}", session_id, reason);
                        }
                        _ => {}
                    }
                }
            });
            
            // 发送测试消息
            let test_messages = vec![
                "Hello, WebSocket Echo Server!",
                "WebSocket中文测试消息",
                "WebSocket JSON: {\"type\":\"test\",\"id\":123}",
                "WebSocket with emojis: 🚀🌟💻",
            ];
            
            for (i, message) in test_messages.iter().enumerate() {
                let packet = Packet::data((i + 1) as u32, message.as_bytes());
                
                println!("📤 发送WebSocket消息 #{}: \"{}\"", i + 1, message);
                
                match transport_clone.send_to_session(session_id, packet).await {
                    Ok(()) => println!("   ✅ WebSocket发送成功"),
                    Err(e) => println!("   ❌ WebSocket发送失败: {:?}", e),
                }
                
                sleep(Duration::from_millis(500)).await;
            }
            
            // 等待响应
            println!("\n⏳ 等待WebSocket服务器回显...");
            sleep(Duration::from_secs(2)).await;
            
            println!("\n🎉 WebSocket Echo测试完成！");
        }
        Err(e) => {
            println!("❌ WebSocket连接失败: {:?}", e);
            println!("💡 提示: 请先启动Echo服务器: cargo run --example echo_server");
        }
    }
    
    Ok(())
}
