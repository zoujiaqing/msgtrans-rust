/// QUIC Echo客户端 - 使用标准化transport.connect()接口

use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    Builder, Config, Event, Packet,
    protocol::adapter::QuicConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    
    println!("🌟 msgtrans QUIC Echo客户端");
    println!("==========================");
    
    let config = Config::default();
    let transport = Builder::new().config(config).build().await?;
    
    // 连接到服务器 - 使用标准化接口
    // 注意：对于QUIC客户端，bind_address实际上是目标服务器地址
    println!("🔌 连接到QUIC Echo服务器: 127.0.0.1:8003");
    let quic_config = QuicConfig::new("127.0.0.1:8003")? // 目标服务器地址
        .with_max_idle_timeout(Duration::from_secs(30));
    
    match transport.connect(quic_config).await {
        Ok(session_id) => {
            println!("✅ QUIC连接建立成功 (SessionId: {})", session_id);
            
            // 启动事件监听
            let mut events = transport.events();
            let transport_clone = transport.clone();
            
            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    match event {
                        Event::MessageReceived { session_id, packet } => {
                            println!("📨 收到QUIC回显 (会话{}):", session_id);
                            if let Some(content) = packet.payload_as_string() {
                                println!("   内容: \"{}\"", content);
                            }
                            println!("   ✅ QUIC回显接收成功");
                        }
                        Event::ConnectionClosed { session_id, reason } => {
                            println!("❌ QUIC连接关闭: 会话{}, 原因: {:?}", session_id, reason);
                        }
                        _ => {}
                    }
                }
            });
            
            // 发送测试消息
            let test_messages = vec![
                "Hello, QUIC Echo Server!",
                "QUIC中文测试消息",
                "QUIC with numbers: 12345",
                "QUIC final message 🚀",
            ];
            
            for (i, message) in test_messages.iter().enumerate() {
                let packet = Packet::data((i + 1) as u32, message.as_bytes());
                
                println!("📤 发送QUIC消息 #{}: \"{}\"", i + 1, message);
                
                match transport_clone.send_to_session(session_id, packet).await {
                    Ok(()) => println!("   ✅ QUIC发送成功"),
                    Err(e) => println!("   ❌ QUIC发送失败: {:?}", e),
                }
                
                sleep(Duration::from_millis(500)).await;
            }
            
            // 等待响应
            println!("\n⏳ 等待QUIC服务器回显...");
            sleep(Duration::from_secs(2)).await;
            
            println!("\n🎉 QUIC Echo测试完成！");
        }
        Err(e) => {
            println!("❌ QUIC连接失败: {:?}", e);
            println!("💡 提示: 请先启动Echo服务器: cargo run --example echo_server");
        }
    }
    
    Ok(())
} 