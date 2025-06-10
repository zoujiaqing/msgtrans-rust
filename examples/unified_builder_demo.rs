/// 统一Builder API演示
/// 
/// 展示修复后的TransportClientBuilder和TransportServerBuilder支持with_protocol的统一设计

use std::time::Duration;
use msgtrans::{
    transport::{
        TransportClientBuilder, TransportServerBuilder,
        ConnectionPoolConfig, RetryConfig, CircuitBreakerConfig,
        RateLimiterConfig, LoggingMiddleware,
    },
    protocol::{TcpClientConfig, TcpServerConfig, WebSocketServerConfig, QuicServerConfig},
    error::TransportError,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🚀 统一Builder API演示");
    
    // 演示客户端 - 单协议配置
    demo_client_unified_api().await?;
    
    // 演示服务端 - 多协议配置
    demo_server_unified_api().await?;
    
    // 演示设计理念
    demo_design_philosophy().await?;
    
    Ok(())
}

/// 演示客户端统一API - 单协议设计
async fn demo_client_unified_api() -> Result<(), TransportError> {
    println!("\n🔌 客户端统一API演示 (单协议设计)");
    
    // 方式1: 流式配置 - Builder级别的协议注册
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .connection_pool(ConnectionPoolConfig {
            max_size: 100,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(60),
            min_idle: 5,
        })
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .with_protocol(TcpClientConfig::new())
        .build()
        .await?;
    
    println!("✅ 客户端构建成功 (预配置TCP协议)");
    
    // 方式2: 传统方式 - 运行时协议指定
    let client2 = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .await?;
    
    println!("✅ 客户端构建成功 (无预配置协议)");
    
    // 展示两种连接方式
    println!("\n📡 连接方式演示:");
    println!("   方式1 (预配置): client.connect().await // 使用构建时配置的协议");
    println!("   方式2 (动态): client.with_protocol(config).connect().await // 运行时指定协议");
    
    Ok(())
}

/// 演示服务端统一API - 多协议设计
async fn demo_server_unified_api() -> Result<(), TransportError> {
    println!("\n🚀 服务端统一API演示 (多协议设计)");
    
    // 服务端支持同时配置多个协议
    let server = TransportServerBuilder::new()
        .bind_timeout(Duration::from_secs(3))
        .max_connections(5000)
        .acceptor_threads(8)
        .rate_limiter(RateLimiterConfig {
            requests_per_second: 1000,
            burst_size: 100,
            window_size: Duration::from_secs(1),
        })
        .with_middleware(LoggingMiddleware::new())
        .with_protocol(
            TcpServerConfig::new()
                .with_bind_address("127.0.0.1:8080".parse().unwrap())
                .with_max_connections(1000)
        )
        .with_protocol(
            WebSocketServerConfig::new()
                .with_bind_address("127.0.0.1:8081".parse().unwrap())
                .with_path("/ws")
                .with_max_connections(500)
        )
        .with_protocol(
            QuicServerConfig::new()
                .with_bind_address("127.0.0.1:4433".parse().unwrap())
                .with_max_connections(2000)
        )
        .build()
        .await?;
    
    println!("✅ 服务端构建成功 (预配置TCP+WebSocket+QUIC协议)");
    
    // 展示服务启动方式
    println!("\n🌐 服务启动方式演示:");
    println!("   方式1 (批量): server.serve_all().await // 启动所有预配置协议");
    println!("   方式2 (单个): server.with_protocol(config).serve().await // 启动单个协议");
    println!("   方式3 (选择): server.serve_multiple(vec![config1, config2]).await // 启动指定协议");
    
    Ok(())
}

/// 演示设计理念
async fn demo_design_philosophy() -> Result<(), TransportError> {
    println!("\n🎯 统一设计理念演示");
    
    println!("📋 核心设计原则:");
    println!("1. 配置驱动: 协议通过配置对象注册，无硬编码");
    println!("2. 意图明确: Client单协议，Server多协议");
    println!("3. 灵活性: 支持构建时预配置 + 运行时动态指定");
    println!("4. 一致性: 统一的 with_protocol() API");
    
    println!("\n🔧 使用场景对比:");
    
    println!("\n📱 客户端场景:");
    println!("   • 移动应用: 根据网络条件切换协议 (WiFi用TCP，4G用QUIC)");
    println!("   • 游戏客户端: 根据延迟要求选择协议");
    println!("   • IoT设备: 根据功耗要求选择协议");
    
    println!("\n🖥️ 服务端场景:");
    println!("   • Web服务器: 同时支持HTTP/WebSocket/QUIC");
    println!("   • 游戏服务器: TCP用于可靠传输，UDP用于实时数据");
    println!("   • 微服务: gRPC、REST、消息队列多协议支持");
    
    println!("\n💡 API设计优势:");
    println!("   • 类型安全: 编译时检查协议配置正确性");
    println!("   • 零成本抽象: 编译时协议分发，无运行时开销");
    println!("   • 易于测试: 每个协议可独立测试");
    println!("   • 易于扩展: 新协议只需实现trait");
    
    println!("\n🔄 向后兼容:");
    println!("   • 原有API继续工作");
    println!("   • 新代码使用新API");
    println!("   • 渐进式迁移路径");
    
    Ok(())
}

/// 展示完整的使用示例（概念性）
#[allow(dead_code)]
async fn demo_complete_usage_example() -> Result<(), TransportError> {
    println!("\n🎨 完整使用示例");
    
    // 构建预配置的客户端
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .with_protocol(TcpClientConfig {
            target_address: "127.0.0.1:8080".parse().unwrap(),
            connect_timeout: Duration::from_secs(5),
            keep_alive: true,
            no_delay: true,
        })
        .build()
        .await?;
    
    // 构建多协议服务端
    let server = TransportServerBuilder::new()
        .max_connections(5000)
        .with_protocol(TcpServerConfig {
            bind_address: "127.0.0.1:8080".parse().unwrap(),
            bind_timeout: Duration::from_secs(3),
            max_connections: 1000,
            keep_alive: true,
        })
        .with_protocol(WebSocketServerConfig {
            bind_address: "127.0.0.1:8081".parse().unwrap(),
            bind_timeout: Duration::from_secs(3),
            max_connections: 500,
            max_frame_size: 1024 * 1024,
        })
        .build()
        .await?;
    
    println!("🎯 实际使用代码:");
    println!("   // 客户端连接");
    println!("   let session = client.connect().await?; // 使用预配置协议");
    println!("   let session = client.with_protocol(other_config).connect().await?; // 动态协议");
    
    println!("\n   // 服务端启动");
    println!("   let server_ids = server.serve_all().await?; // 启动所有协议");
    println!("   let server_id = server.with_protocol(specific_config).serve().await?; // 单协议");
    
    println!("\n   // 统一的事件处理");
    println!("   while let Some(event) = client.events().next().await {{");
    println!("       match event {{");
    println!("           Event::Connected(session_id) => {{ /* 处理连接 */ }}");
    println!("           Event::DataReceived(session_id, packet) => {{ /* 处理数据 */ }}");
    println!("           Event::Disconnected(session_id) => {{ /* 处理断开 */ }}");
    println!("       }}");
    println!("   }}");
    
    Ok(())
} 