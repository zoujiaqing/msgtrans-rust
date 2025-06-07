/// TCP Echo客户端 - 连接到Echo服务器进行测试

use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    Builder, Config, Event, Packet,
    protocol::adapter::TcpConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    
    println!("🌟 msgtrans TCP Echo客户端");
    println!("=======================");
    
    let config = Config::default();
    let transport = Builder::new().config(config).build().await?;
    
    // 连接到服务器
    println!("🔌 连接到TCP Echo服务器: 127.0.0.1:8001");
    let tcp_config = TcpConfig::new("127.0.0.1:8001")?.with_nodelay(true);
    
    match transport.connect(tcp_config).await {
        Ok(session_id) => {
            println!("✅ 连接建立成功 (SessionId: {})", session_id);
            
            // 启动事件监听
            let mut events = transport.events();
            let transport_clone = transport.clone();
            
            tokio::spawn(async move {
                while let Some(event) = events.next().await {
                    match event {
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
                "Hello, TCP Echo Server!",
                "这是中文测试消息",
                "Message with numbers: 12345",
                "Special chars: !@#$%^&*()",
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
            sleep(Duration::from_secs(2)).await;
            
            println!("\n🎉 TCP Echo测试完成！");
        }
        Err(e) => {
            println!("❌ 连接失败: {:?}", e);
            println!("💡 提示: 请先启动Echo服务器: cargo run --example echo_server");
        }
    }
    
    Ok(())
} 