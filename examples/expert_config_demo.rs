/// 专家配置演示
/// 
/// 展示如何使用专家级Builder配置来优化传输性能

use msgtrans::transport::{
    TransportBuilder, SmartPoolConfig, PerformanceConfig
};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 专家配置演示");
    println!("================");
    
    // 1. 使用预设配置
    println!("\n📊 1. 高性能预设配置");
    let _transport_hp = TransportBuilder::new()
        .high_performance()
        .build()
        .await?;
    
    // 2. 自定义智能连接池配置
    println!("\n⚙️ 2. 自定义智能连接池配置");
    let custom_pool_config = SmartPoolConfig {
        initial_size: 150,
        max_size: 3000,
        expansion_threshold: 0.85,
        shrink_threshold: 0.25,
        expansion_cooldown: Duration::from_secs(20),
        shrink_cooldown: Duration::from_secs(90),
        min_connections: 20,
        enable_warmup: true,
        expansion_factors: vec![3.0, 2.0, 1.5, 1.2, 1.1],
    };
    
    let _transport_custom = TransportBuilder::new()
        .with_smart_pool_config(custom_pool_config)
        .build()
        .await?;
    
    println!("\n🎉 专家配置演示完成！");
    
    Ok(())
}
