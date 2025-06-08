/// WebSocket Echo客户端 - 展示统一connect API
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    transport::TransportBuilder,
    protocol::WebSocketConfig,
    Event, Packet,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🌟 WebSocket Echo客户端（统一API）");
    println!("=================================");
    
    // 🎯 创建传输实例
    let transport = TransportBuilder::new().build().await?;
    
    // 🔌 统一连接方法 - 传入协议配置即可
    println!("🔌 连接到WebSocket Echo服务器: 127.0.0.1:8002");
    let ws_config = WebSocketConfig {
        bind_address: "127.0.0.1:8002".parse()?,
        ..Default::default()
    };
    
    match transport.connect(ws_config).await {
        Ok(session_id) => {
            println!("✅ 连接建立成功 (SessionId: {})", session_id);
            
            // 启动事件监听
            let mut events = transport.events();
            let transport_clone = transport.clone();
            
            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    match event {
                        Event::ConnectionEstablished { session_id, info } => {
                            println!("🔗 连接已建立: {} [{:?}]", session_id, info.protocol);
                        }
                        Event::MessageReceived { session_id, packet } => {
                            println!("📨 收到回显 (会话{}):", session_id);
                            if let Some(content) = packet.payload_as_string() {
                                println!("   内容: \"{}\"", content);
                            }
                            println!("   ✅ 回显接收成功");
                        }
                        Event::ConnectionClosed { session_id, reason } => {
                            println!("❌ 连接关闭: 会话{}, 原因: {:?}", session_id, reason);
                        }
                        _ => {}
                    }
                }
            });
            
            // 发送测试消息
            let test_messages = vec![
                "Hello, WebSocket Echo Server!",
                "这是中文测试消息",
                "WebSocket JSON: {\"type\":\"test\",\"id\":123}",
                "WebSocket with emojis: 🚀🌟💻",
            ];
            
            for (i, message) in test_messages.iter().enumerate() {
                let packet = Packet::data((i + 1) as u32, message.as_bytes());
                
                println!("📤 发送消息 #{}: \"{}\"", i + 1, message);
                
                match transport_clone.send_to_session(session_id, packet).await {
                    Ok(()) => println!("   ✅ 发送成功"),
                    Err(e) => println!("   ❌ 发送失败: {:?}", e),
                }
                
                sleep(Duration::from_millis(500)).await;
            }
            
            // 等待响应
            println!("\n⏳ 等待服务器回显...");
            sleep(Duration::from_secs(3)).await;
            
            println!("\n🎉 WebSocket Echo测试完成！");
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请先启动Echo服务器: cargo run --example echo_server");
        }
    }
    
    Ok(())
}
