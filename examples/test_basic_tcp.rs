/// 最基础的TCP连接测试
/// 
/// 绕过统一架构，直接测试TCP适配器层

use std::time::Duration;
use tokio::time::sleep;

use msgtrans::unified::{
    adapters::tcp::{TcpServerBuilder, TcpClientBuilder},
    adapter::TcpConfig,
    packet::UnifiedPacket,
    adapter::ProtocolAdapter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
    
    println!("🔧 基础TCP适配器测试");
    println!("==================");
    
    // 创建TCP服务器
    let config = TcpConfig::default();
    let bind_addr = "127.0.0.1:8090".parse()?;
    
    let mut server = TcpServerBuilder::new()
        .bind_address(bind_addr)
        .config(config.clone())
        .build()
        .await?;
    
    println!("✅ TCP服务器创建成功: {}", server.local_addr()?);
    
    // 在后台接受连接
    tokio::spawn(async move {
        println!("🔍 服务器开始等待连接...");
        
        match server.accept().await {
            Ok(mut adapter) => {
                println!("🔍 服务器接受到连接: 会话{}", adapter.session_id());
                
                // 接收消息
                while let Ok(Some(packet)) = adapter.receive().await {
                    println!("🔍 服务器收到消息: {:?}", packet);
                    
                    // 发送回显
                    let echo_content = format!("Echo: {}", String::from_utf8_lossy(&packet.payload));
                    let echo_packet = UnifiedPacket::echo(packet.message_id, echo_content.as_bytes());
                    
                    if let Err(e) = adapter.send(echo_packet).await {
                        println!("❌ 服务器发送回显失败: {:?}", e);
                    } else {
                        println!("✅ 服务器发送回显成功");
                    }
                }
                
                println!("🔍 服务器连接关闭");
            }
            Err(e) => {
                println!("❌ 服务器接受连接失败: {:?}", e);
            }
        }
    });
    
    // 等待服务器启动
    sleep(Duration::from_millis(100)).await;
    
    // 创建TCP客户端
    println!("🔍 创建TCP客户端...");
    
    let mut client = TcpClientBuilder::new()
        .target_address(bind_addr)
        .config(config)
        .connect()
        .await?;
    
    println!("✅ TCP客户端连接成功: 会话{}", client.session_id());
    
    // 发送测试消息
    let test_message = "Hello from basic TCP test!";
    let packet = UnifiedPacket::data(1, test_message.as_bytes());
    
    println!("📤 客户端发送消息: \"{}\"", test_message);
    client.send(packet).await?;
    
    // 接收回显
    println!("⏳ 客户端等待回显...");
    
    match client.receive().await? {
        Some(response) => {
            let content = String::from_utf8_lossy(&response.payload);
            println!("📨 客户端收到回显: \"{}\"", content);
            println!("✅ 基础TCP测试成功！");
        }
        None => {
            println!("❌ 客户端未收到回显");
        }
    }
    
    // 关闭连接
    client.close().await?;
    
    println!("👋 基础TCP测试结束");
    
    Ok(())
} 