/// 分离式Builder使用示例
/// 
/// 展示新的ClientTransport和ServerTransport API

use std::time::Duration;
use msgtrans::{
    transport::{
        TransportClientBuilder, TransportServerBuilder,
        ConnectionPoolConfig, RetryConfig, CircuitBreakerConfig,
        RateLimiterConfig, LoggingMiddleware, AcceptorConfig,
        TcpConfig,
    },
    error::TransportError,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::init();
    
    println!("🚀 分离式Builder示例");
    
    // 演示客户端构建
    demo_client_builder().await?;
    
    // 演示服务端构建  
    demo_server_builder().await?;
    
    // 演示流式API
    demo_fluent_api().await?;
    
    Ok(())
}

/// 演示客户端构建器
async fn demo_client_builder() -> Result<(), TransportError> {
    println!("\n🔌 客户端传输层构建示例");
    
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .connection_pool(ConnectionPoolConfig {
            max_size: 100,
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(60),
            min_idle: 5,
        })
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .circuit_breaker(CircuitBreakerConfig {
            failure_threshold: 10,
            timeout: Duration::from_secs(60),
            success_threshold: 3,
        })
        .enable_connection_monitoring(true)
        .build()
        .await?;
    
    println!("✅ 客户端传输层构建成功");
    
    // 获取统计信息
    let stats = client.stats().await?;
    println!("📊 当前连接统计: {} 个会话", stats.len());
    
    Ok(())
}

/// 演示服务端构建器
async fn demo_server_builder() -> Result<(), TransportError> {
    println!("\n🚀 服务端传输层构建示例");
    
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
        .graceful_shutdown(Some(Duration::from_secs(30)))
        .build()
        .await?;
    
    println!("✅ 服务端传输层构建成功");
    
    // 获取服务器状态
    let server_count = server.server_count().await;
    println!("📊 当前运行服务器: {} 个", server_count);
    
    Ok(())
}

/// 演示流式API
async fn demo_fluent_api() -> Result<(), TransportError> {
    println!("\n🎯 流式API示例");
    
    // 构建客户端
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .build()
        .await?;
    
    // 构建服务端
    let server = TransportServerBuilder::new()
        .max_connections(1000)
        .with_middleware(LoggingMiddleware::new())
        .build()
        .await?;
    
    println!("✅ 传输层准备完成");
    
    // 注意：实际的连接需要有效的TcpConfig实现，这里只是展示API结构
    println!("💡 可以使用以下流式API进行连接：");
    println!("   client.with_protocol(tcp_config).with_timeout(Duration::from_secs(5)).connect().await");
    println!("   server.with_protocol(tcp_config).with_name(\"my-server\".to_string()).serve().await");
    
    // 展示批量连接（需要有效配置）
    // let sessions = client.connect_multiple(vec![tcp_config1, tcp_config2]).await?;
    // println!("📦 批量连接建立: {:?}", sessions);
    
    // 展示多协议服务器（需要有效配置）
    // let server_ids = server.serve_multiple(vec![tcp_config, quic_config]).await?;
    // println!("🌐 多协议服务器启动: {:?}", server_ids);
    
    Ok(())
}

/// 展示自定义协议配置的概念
fn demo_custom_protocol_concept() {
    println!("\n🔧 自定义协议支持概念");
    
    println!("自定义协议只需要实现三个trait：");
    println!("1. ProtocolConfig - 纯配置信息");
    println!("2. ConnectableConfig - 客户端连接行为");
    println!("3. ServerConfig - 服务端监听行为");
    
    println!("\n使用方式完全一致：");
    println!("client.with_protocol(my_custom_config).connect().await");
    println!("server.with_protocol(my_custom_config).serve().await");
} 