/// QUIC Echo客户端 - 连接到Echo服务器进行测试
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

use msgtrans::{
    Builder, Config, Event, Packet,
    protocol::adapter::QuicConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化TLS加密提供者（QUIC需要）
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| "Failed to install crypto provider")?;
    
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .init();
    
    println!("🌟 msgtrans QUIC Echo客户端");
    println!("=========================");
    
    let config = Config::default();
    let transport = Builder::new().config(config).build().await?;
    
    // 连接到服务器
    println!("🔌 连接到QUIC Echo服务器: 127.0.0.1:8003");
    let quic_config = QuicConfig::new("127.0.0.1:8003")?
        .with_max_idle_timeout(Duration::from_secs(30))
        .with_keep_alive_interval(Some(Duration::from_secs(10)))
        .with_max_concurrent_streams(10);
    
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
                "QUIC high-performance message",
                "QUIC with low latency: 🚀",
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
            println!("💡 QUIC特性:");
            println!("   🚀 低延迟连接建立");
            println!("   🔒 内置TLS加密");
            println!("   🌊 多路复用流");
            println!("   📦 高效数据传输");
        }
        Err(e) => {
            println!("❌ QUIC连接失败: {:?}", e);
            println!("💡 提示:");
            println!("   1. 请先启动Echo服务器: cargo run --example echo_server");
            println!("   2. QUIC需要TLS证书，服务器可能使用自签名证书");
            println!("   3. 某些网络环境可能阻止QUIC协议");
        }
    }
    
    Ok(())
} 