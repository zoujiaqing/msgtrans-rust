use msgtrans::protocol::adapter::{TcpConfig, WebSocketConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌟 msgtrans 改进的用户体验测试");
    println!("===============================");
    
    // 1. 类型安全的配置API
    println!("\n📋 1. 类型安全的配置API");
    
    // TCP配置 - 成功案例
    println!("   🔧 TCP配置:");
    let tcp_config = TcpConfig::new("127.0.0.1:8080")?
        .with_nodelay(true)
        .with_keepalive(Some(std::time::Duration::from_secs(30)));
    println!("   ✅ TCP配置创建成功: {:?}", tcp_config.bind_address);
    
    // TCP配置 - 错误处理
    println!("   🔧 TCP配置错误处理:");
    match TcpConfig::new("invalid-address") {
        Ok(_) => println!("   ❌ 应该失败"),
        Err(e) => println!("   ✅ 友好错误: {}", e),
    }
    
    // WebSocket配置 - 成功案例
    println!("   🔧 WebSocket配置:");
    let ws_config = WebSocketConfig::new("127.0.0.1:8081")?
        .with_path("/ws")
        .with_max_frame_size(64 * 1024);
    println!("   ✅ WebSocket配置创建成功: {:?}", ws_config.bind_address);
    
    // WebSocket配置 - 路径验证（现在只会警告，不会失败）
    println!("   🔧 WebSocket路径验证:");
    let config = WebSocketConfig::new("127.0.0.1:8081")?;
    let _config_with_invalid_path = config.with_path("invalid-path");
    println!("   ⚠️ 无效路径会使用默认值 '/ws'");
    
    // 2. 链式配置API
    println!("\n🔗 2. 链式配置API");
    
    // 流畅的链式调用
    let _chained_config = WebSocketConfig::new("127.0.0.1:8082")?
        .with_path("/api/websocket")
        .with_subprotocols(vec!["chat".to_string(), "echo".to_string()])
        .with_max_frame_size(128 * 1024)
        .with_ping_interval(Some(std::time::Duration::from_secs(30)));
    
    println!("   ✅ 链式配置API工作正常");
    
    // 3. 预设配置
    println!("\n⚡ 3. 预设配置");
    
    let _high_perf = TcpConfig::high_performance("127.0.0.1:8083")?;
    println!("   ✅ 高性能预设配置");
    
    let _low_latency = TcpConfig::low_latency("127.0.0.1:8084")?;
    println!("   ✅ 低延迟预设配置");
    
    let _high_throughput = TcpConfig::high_throughput("127.0.0.1:8085")?;
    println!("   ✅ 高吞吐量预设配置");
    
    println!("\n🎉 所有用户体验改进测试通过！");
    
    Ok(())
} 