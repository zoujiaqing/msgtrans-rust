/// 分离式Builder演示
/// 
/// 展示新的TransportClientBuilder和TransportServerBuilder API

use std::time::Duration;
use msgtrans::{
    transport::{
        TransportClientBuilder, TransportServerBuilder,
        ConnectionPoolConfig, RetryConfig, CircuitBreakerConfig,
        RateLimiterConfig, LoggingMiddleware,
    },
    error::TransportError,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();
    
    println!("🚀 分离式Builder演示");
    
    // 演示客户端构建
    demo_client_builder().await?;
    
    // 演示服务端构建  
    demo_server_builder().await?;
    
    // 演示API设计优势
    demo_api_advantages().await?;
    
    Ok(())
}

/// 演示客户端构建器
async fn demo_client_builder() -> Result<(), TransportError> {
    println!("\n🔌 客户端传输层构建演示");
    
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
    
    let active_sessions = client.active_sessions().await?;
    println!("🔗 活跃会话: {} 个", active_sessions.len());
    
    Ok(())
}

/// 演示服务端构建器
async fn demo_server_builder() -> Result<(), TransportError> {
    println!("\n🚀 服务端传输层构建演示");
    
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
    
    let active_servers = server.active_servers().await;
    println!("🌐 活跃服务器: {:?}", active_servers);
    
    Ok(())
}

/// 演示API设计优势
async fn demo_api_advantages() -> Result<(), TransportError> {
    println!("\n🎯 API设计优势演示");
    
    // 1. 意图明确 - 从类型就知道用途
    println!("1. 意图明确:");
    println!("   TransportClientBuilder -> 明确是客户端");
    println!("   TransportServerBuilder -> 明确是服务端");
    
    // 2. 专业配置 - 每个Builder只有相关配置
    println!("\n2. 专业配置:");
    println!("   客户端: connect_timeout, retry_strategy, circuit_breaker");
    println!("   服务端: bind_timeout, max_connections, rate_limiter");
    
    // 3. 流式API设计
    println!("\n3. 流式API设计:");
    println!("   client.with_protocol(config).with_timeout(5s).connect()");
    println!("   server.with_protocol(config).with_name(\"my-server\").serve()");
    
    // 4. 类型安全
    println!("\n4. 类型安全:");
    println!("   编译时确保正确的配置类型和使用方式");
    
    // 5. 易于扩展
    println!("\n5. 易于扩展:");
    println!("   新协议只需实现trait，无需修改核心代码");
    
    // 6. 向下兼容
    println!("\n6. 向下兼容:");
    println!("   原有API可以继续工作，新代码使用新API");
    
    Ok(())
}

/// 展示流式API概念（需要实际的协议配置才能运行）
#[allow(dead_code)]
async fn demo_fluent_api_concept() -> Result<(), TransportError> {
    println!("\n💡 流式API概念演示");
    
    // 构建传输层
    let client = TransportClientBuilder::new()
        .connect_timeout(Duration::from_secs(10))
        .with_protocol(QuicClientConfig::default())
        .retry_strategy(RetryConfig::exponential_backoff(3, Duration::from_millis(100)))
        .build()
        .await?;
    
    let server = TransportServerBuilder::new()
        .max_connections(1000)
        .with_middleware(LoggingMiddleware::new())
        .with_protocol(QuicServerConfig::default())
        .with_protocol(TcpServerConfig::default())
        .with_protocol(WebSocketServerConfig::default())
        .build()
        .await?;
    
    println!("✅ 传输层准备完成");
    
    // 注意：以下代码需要有效的协议配置才能实际运行
    println!("💡 流式API使用示例（概念）:");
    println!("   // 客户端连接");
    println!("   let session = client");
    println!("       .with_protocol(tcp_config)");
    println!("       .with_timeout(Duration::from_secs(5))");
    println!("       .with_retry(3)");
    println!("       .connect().await?;");
    
    println!("\n   // 服务端启动");
    println!("   let server_id = server");
    println!("       .with_protocol(tcp_config)");
    println!("       .with_name(\"tcp-echo-server\".to_string())");
    println!("       .with_max_connections(1000)");
    println!("       .serve().await?;");
    
    println!("\n   // 批量操作");
    println!("   let sessions = client.connect_multiple(vec![config1, config2]).await?;");
    println!("   let server_ids = server.serve_multiple(vec![tcp_config, quic_config]).await?;");
    
    Ok(())
}

/// 展示自定义协议支持概念
#[allow(dead_code)]
fn demo_custom_protocol_concept() {
    println!("\n🔧 自定义协议支持概念");
    
    println!("自定义协议只需要实现三个trait：");
    println!("1. ProtocolConfig - 纯配置信息");
    println!("   - validate() - 配置验证");
    println!("   - protocol_name() - 协议名称");
    println!("   - default_config() - 默认配置");
    
    println!("\n2. ConnectableConfig - 客户端连接行为");
    println!("   - connect(&self, transport) - 建立连接");
    
    println!("\n3. ServerConfig - 服务端监听行为");
    println!("   - build_server(&self) - 构建服务器");
    println!("   - validate(&self) - 配置验证");
    
    println!("\n使用方式完全一致：");
    println!("client.with_protocol(my_custom_config).connect().await");
    println!("server.with_protocol(my_custom_config).serve().await");
    
    println!("\n✨ 这样设计的好处：");
    println!("- 配置和行为分离，符合单一职责原则");
    println!("- 类型安全，编译时检查");
    println!("- 易于测试和维护");
    println!("- 支持任意自定义协议");
} 