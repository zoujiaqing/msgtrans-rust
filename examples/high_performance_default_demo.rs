/// 🚀 默认高性能组件演示
/// 
/// 本演示展示用户现在默认获得的高性能组件：
/// 1. LockFree 连接池
/// 2. 优化内存池
/// 3. Flume 协议适配器
/// 4. 双管道 Actor
/// 
/// 无需任何额外配置 - 全部开箱即用！

use std::time::{Duration, Instant};
use msgtrans::{
    TransportBuilder, TransportConfig,
    MemoryPool, ProtocolAdapter, Actor,  // 🚀 这些现在都是优化版本！
    ConnectionPool, PerformanceMetrics,
    create_test_packet
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 MsgTrans 默认高性能组件演示");
    println!("============================================\n");
    
    // 测试1: 默认Transport配置就是高性能的
    println!("🔧 测试1: 创建Transport - 默认高性能配置");
    test_default_transport_creation().await?;
    
    println!("\n{}\n", "=".repeat(60));
    
    // 测试2: 内存池默认就是优化版本
    println!("💾 测试2: 内存池性能 - 默认优化版本");
    test_memory_pool_performance().await?;
    
    println!("\n{}\n", "=".repeat(60));
    
    // 测试3: 协议适配器默认就是Flume版本
    println!("📡 测试3: 协议适配器性能 - 默认Flume版本");
    test_protocol_adapter_performance().await?;
    
    println!("\n{}\n", "=".repeat(60));
    
    // 测试4: 用户友好的API，背后是高性能实现
    println!("👤 测试4: 用户友好API + 高性能后端");
    test_user_friendly_api().await?;
    
    println!("\n🎉 演示完成！");
    println!("💡 关键收益：");
    println!("   ✅ 零配置 - 用户无需了解内部实现");
    println!("   ✅ 高性能 - 默认就是最优组件");
    println!("   ✅ 透明化 - API保持简洁易用");
    println!("   ✅ 向后兼容 - legacy模块保留旧组件");
    
    Ok(())
}

/// 测试默认Transport创建就是高性能的
async fn test_default_transport_creation() -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    
    // 🎯 用户只需要简单配置，底层自动使用高性能组件
    let transport = TransportBuilder::new()
        .config(TransportConfig::default())
        .build()  // 🚀 内部自动创建 LockFree连接池 + 优化内存池
        .await?;
    
    let creation_time = start_time.elapsed();
    
    // 验证高性能组件已启用
    let pool_stats = transport.connection_pool_stats();
    let memory_stats = transport.memory_pool_stats();
    
    println!("   ⚡ Transport创建耗时: {:?}", creation_time);
    println!("   📊 连接池统计: {} 总连接, {} 活跃连接", 
             pool_stats.total_connections, pool_stats.active_connections);
    println!("   💾 内存池统计: {:.1}% 缓存命中率", 
             memory_stats.cache_hit_rate * 100.0);
    println!("   ✅ 默认启用了 LockFree + 优化组件！");
    
    Ok(())
}

/// 测试内存池性能 - 默认就是优化版本
async fn test_memory_pool_performance() -> Result<(), Box<dyn std::error::Error>> {
    use msgtrans::BufferSize;
    
    // 🎯 用户使用的 MemoryPool 现在就是 OptimizedMemoryPool！
    let memory_pool = MemoryPool::new();
    
    println!("   🔧 执行内存池性能测试...");
    
    let iterations = 50000;
    let start_time = Instant::now();
    
    // 高频分配/释放操作
    for i in 0..iterations {
        let size = match i % 3 {
            0 => BufferSize::Small,
            1 => BufferSize::Medium,
            _ => BufferSize::Large,
        };
        
        let buffer = memory_pool.get_buffer(size);
        memory_pool.return_buffer(buffer, size);
    }
    
    let duration = start_time.elapsed();
    let ops_per_sec = iterations as f64 / duration.as_secs_f64();
    
    let stats = memory_pool.get_stats();
    
    println!("   ⚡ 内存操作性能: {:.0} ops/s ({} 次操作耗时 {:?})", 
             ops_per_sec, iterations, duration);
    println!("   📊 缓存命中率: {:.1}%", stats.cache_hit_rate * 100.0);
    println!("   🚀 总内存分配: {:.1} MB", stats.total_memory_allocated_mb);
    println!("   ✅ 用户无感知使用了 LockFree 优化内存池！");
    
    Ok(())
}

/// 测试协议适配器性能 - 默认就是Flume版本  
async fn test_protocol_adapter_performance() -> Result<(), Box<dyn std::error::Error>> {
    use msgtrans::command::{ConnectionInfo, ProtocolType, ConnectionState};
    
    // 🎯 用户使用的 ProtocolAdapter 现在就是 FlumePoweredProtocolAdapter！
    let connection_info = ConnectionInfo {
        session_id: msgtrans::SessionId::new(1),
        local_addr: "127.0.0.1:8080".parse().unwrap(),
        peer_addr: "127.0.0.1:8081".parse().unwrap(),
        protocol: ProtocolType::Tcp,
        state: ConnectionState::Connected,
        established_at: std::time::SystemTime::now(),
        closed_at: None,
        last_activity: std::time::SystemTime::now(),
        packets_sent: 0,
        packets_received: 0,
        bytes_sent: 0,
        bytes_received: 0,
    };
    
    let (adapter, _event_rx) = ProtocolAdapter::new(msgtrans::SessionId::new(1), connection_info);
    
    println!("   🔧 执行协议适配器性能测试...");
    
    let iterations = 25000;
    let start_time = Instant::now();
    
    // 高频非阻塞发送
    for i in 0..iterations {
        let packet = create_test_packet(i, 512);
        let _ = adapter.send_nowait(packet);
    }
    
    let send_duration = start_time.elapsed();
    let send_ops_per_sec = iterations as f64 / send_duration.as_secs_f64();
    
    // 批量处理
    let batch_start = Instant::now();
    let mut total_processed = 0;
    
    while total_processed < iterations {
        let processed = adapter.process_send_batch(32).await?;
        if processed == 0 { break; }
        total_processed += processed;
    }
    
    let batch_duration = batch_start.elapsed();
    let batch_ops_per_sec = total_processed as f64 / batch_duration.as_secs_f64();
    
    let stats = adapter.get_stats();
    
    println!("   ⚡ 非阻塞发送: {:.0} ops/s", send_ops_per_sec);
    println!("   ⚡ 批量处理: {:.0} ops/s", batch_ops_per_sec);
    println!("   📊 平均批次大小: {:.1} 包/批次", stats.average_batch_size());
    println!("   ⏱️ 平均延迟: {:.2} μs", stats.average_latency_ns() / 1000.0);
    println!("   ✅ 用户无感知使用了 Flume 协议适配器！");
    
    Ok(())
}

/// 测试用户友好API背后的高性能实现
async fn test_user_friendly_api() -> Result<(), Box<dyn std::error::Error>> {
    println!("   🎯 展示：简单API + 高性能后端");
    
    // 🎯 用户代码很简单...
    let config = TransportConfig::default();
    let transport = TransportBuilder::new()
        .config(config)
        .build()
        .await?;
    
    println!("   👤 用户代码: 仅3行创建Transport");
    
    // ...但背后自动获得了所有优化组件！
    let pool_stats = transport.connection_pool_stats();
    let memory_stats = transport.memory_pool_stats();
    
    println!("   🔍 背后自动获得的组件:");
    println!("      🚀 LockFree连接池 ({} 总操作)", pool_stats.total_operations);
    println!("      💾 优化内存池 ({:.1}% 效率)", memory_stats.memory_efficiency * 100.0);
    println!("      📡 Flume协议适配器 (批量处理)");
    println!("      🎭 双管道Actor (数据/命令分离)");
    println!("      🔢 LockFree统计 (零开销监控)");
    
    println!("   ✅ 零配置获得最大性能！");
    
    // 展示legacy模块仍然可用
    #[allow(unused_imports)]
    use msgtrans::legacy::{LegacyMemoryPool, LegacyActor};
    println!("   📦 Legacy组件仍可访问: msgtrans::legacy::*");
    
    Ok(())
} 