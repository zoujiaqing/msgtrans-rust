/// 统一关闭机制演示
/// 
/// 展示 Transport 层的统一关闭机制：
/// 1. 优雅关闭：发送关闭事件 → 等待超时 → 清理资源
/// 2. 强制关闭：立即关闭，不等待
/// 3. 批量关闭：关闭所有连接
/// 4. 关闭状态管理：防止重复关闭

use std::time::Duration;
use msgtrans::{
    transport::{
        client::TransportClientBuilder,
        server::TransportServerBuilder,
        config::TransportConfig,
    },
    protocol::{TcpClientConfig, TcpServerConfig},
    packet::Packet,
    event::TransportEvent,
};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 启用详细日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🎯 统一关闭机制演示");
    println!("==================");
    println!();

    // 配置较短的优雅关闭超时时间用于演示
    let transport_config = TransportConfig {
        graceful_timeout: Duration::from_secs(2),
        ..Default::default()
    };

    // 1. 启动服务器
    println!("🚀 启动服务器...");
    let tcp_server_config = TcpServerConfig {
        bind_address: "127.0.0.1:8002".parse()?,
        ..Default::default()
    };

    let server = TransportServerBuilder::new()
        .transport_config(transport_config.clone())
        .with_protocol(tcp_server_config)
        .build()
        .await?;

    // 启动服务器监听
    let server_clone = server.clone();
    let server_task = tokio::spawn(async move {
        if let Err(e) = server_clone.serve().await {
            eprintln!("❌ 服务器错误: {:?}", e);
        }
    });

    // 等待服务器启动
    sleep(Duration::from_millis(500)).await;

    // 2. 创建多个客户端连接
    println!("🔌 创建多个客户端连接...");
    let mut clients = Vec::new();
    
    for i in 1..=3 {
        let tcp_client_config = TcpClientConfig {
            target_address: "127.0.0.1:8002".parse()?,
            ..Default::default()
        };

        let mut client = TransportClientBuilder::new()
            .transport_config(transport_config.clone())
            .with_protocol(tcp_client_config)
            .build()
            .await?;

        client.connect().await?;
        println!("✅ 客户端 {} 连接成功", i);
        clients.push(client);
    }

    // 等待连接稳定
    sleep(Duration::from_millis(500)).await;

    // 3. 演示优雅关闭
    println!("\n🎯 演示优雅关闭机制");
    println!("==================");
    
    if let Some(mut client) = clients.pop() {
        println!("🔌 优雅关闭客户端 3...");
        let start = std::time::Instant::now();
        
        match client.disconnect().await {
            Ok(_) => {
                let elapsed = start.elapsed();
                println!("✅ 客户端 3 优雅关闭成功，耗时: {:?}", elapsed);
            }
            Err(e) => {
                println!("❌ 客户端 3 优雅关闭失败: {:?}", e);
            }
        }
    }

    // 4. 演示强制关闭
    println!("\n🎯 演示强制关闭机制");
    println!("==================");
    
    if let Some(mut client) = clients.pop() {
        println!("🔌 强制关闭客户端 2...");
        let start = std::time::Instant::now();
        
        match client.force_disconnect().await {
            Ok(_) => {
                let elapsed = start.elapsed();
                println!("✅ 客户端 2 强制关闭成功，耗时: {:?}", elapsed);
            }
            Err(e) => {
                println!("❌ 客户端 2 强制关闭失败: {:?}", e);
            }
        }
    }

    // 5. 演示服务端批量关闭
    println!("\n🎯 演示服务端批量关闭");
    println!("====================");
    
    // 先查看当前活跃连接
    let active_sessions = server.active_sessions().await;
    println!("📊 当前活跃连接数: {}", active_sessions.len());
    
    if !active_sessions.is_empty() {
        println!("🔌 批量关闭所有服务端连接...");
        let start = std::time::Instant::now();
        
        match server.close_all_sessions().await {
            Ok(_) => {
                let elapsed = start.elapsed();
                println!("✅ 批量关闭成功，耗时: {:?}", elapsed);
                
                // 检查关闭后的连接数
                let remaining_sessions = server.active_sessions().await;
                println!("📊 关闭后剩余连接数: {}", remaining_sessions.len());
            }
            Err(e) => {
                println!("❌ 批量关闭失败: {:?}", e);
            }
        }
    }

    // 6. 演示重复关闭保护
    println!("\n🎯 演示重复关闭保护");
    println!("==================");
    
    if let Some(mut client) = clients.pop() {
        println!("🔌 第一次关闭客户端 1...");
        match client.disconnect().await {
            Ok(_) => println!("✅ 第一次关闭成功"),
            Err(e) => println!("❌ 第一次关闭失败: {:?}", e),
        }
        
        println!("🔌 第二次关闭同一客户端（应该被忽略）...");
        match client.disconnect().await {
            Ok(_) => println!("✅ 第二次关闭被正确处理"),
            Err(e) => println!("❌ 第二次关闭失败: {:?}", e),
        }
    }

    // 7. 停止服务器
    println!("\n🛑 停止服务器...");
    server.stop().await;
    
    // 等待服务器任务结束
    let _ = tokio::time::timeout(Duration::from_secs(3), server_task).await;
    
    println!("✅ 统一关闭机制演示完成");
    Ok(())
} 