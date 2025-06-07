use msgtrans::{
    Transport, Config, Builder,
    protocol::adapter::{TcpConfig, WebSocketConfig},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🌟 msgtrans 改进的API展示");
    println!("========================");
    
    // 1. 类型安全的配置API
    println!("\n📋 1. 类型安全的配置API");
    
    // TCP配置 - 立即验证
    println!("   🔧 TCP配置:");
    let tcp_config = TcpConfig::new("127.0.0.1:8080")?
        .with_nodelay(true)
        .with_keepalive(Some(std::time::Duration::from_secs(30)));
    println!("   ✅ TCP配置创建成功");
    
    // 错误处理演示
    println!("   🔧 错误处理演示:");
    match TcpConfig::new("invalid-address") {
        Ok(_) => println!("   ❌ 不应该成功"),
        Err(e) => println!("   ✅ 地址验证: {}", e),
    }
    
    // WebSocket配置
    println!("   🔧 WebSocket配置:");
    let ws_config = WebSocketConfig::new("127.0.0.1:8081")?
        .with_path("/api/websocket")
        .with_max_frame_size(64 * 1024);
    println!("   ✅ WebSocket配置创建成功");
    
    // 路径验证演示（现在只会警告）
    println!("   🔧 路径验证演示:");
    let config = WebSocketConfig::new("127.0.0.1:8081")?;
    let _config_with_invalid_path = config.with_path("invalid-path");
    println!("   ⚠️ 无效路径会使用默认值");
    
    // 2. 预设配置
    println!("\n⚡ 2. 预设配置");
    
    let _high_perf = TcpConfig::high_performance("127.0.0.1:8082")?;
    println!("   ✅ 高性能预设");
    
    let _low_latency = TcpConfig::low_latency("127.0.0.1:8083")?;
    println!("   ✅ 低延迟预设");
    
    let _high_throughput = TcpConfig::high_throughput("127.0.0.1:8084")?;
    println!("   ✅ 高吞吐量预设");
    
    // 3. 流畅的链式API
    println!("\n🔗 3. 流畅的链式API");
    
    let _complex_config = WebSocketConfig::new("127.0.0.1:8085")?
        .with_path("/api/websocket")
        .with_subprotocols(vec!["chat".to_string(), "echo".to_string()])
        .with_max_frame_size(128 * 1024)
        .with_ping_interval(Some(std::time::Duration::from_secs(30)));
    
    println!("   ✅ 复杂链式配置成功");
    
    // 4. 传输层集成
    println!("\n🚀 4. 传输层集成");
    
    let config = Config::default();
    let _transport = Builder::new()
        .config(config)
        .build()
        .await?;
    
    println!("   ✅ 传输层创建成功");
    
    println!("\n🎉 所有API改进展示完成！");
    
    Ok(())
} 