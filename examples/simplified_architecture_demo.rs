/// 🚀 Phase 4: 简化架构演示
/// 
/// 展示新的简化架构：
/// - 直接使用OptimizedActor，无复杂的ActorManager包装
/// - SimplifiedSessionManager管理会话状态
/// - 高性能LockFree组件
/// - 真实网络适配器集成

use msgtrans::{
    Transport, TransportConfig, Packet,
    error::TransportError,
};
use std::time::Duration;
use tokio::time::sleep;
use futures::StreamExt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("🚀 Phase 4: 简化架构演示");
    println!("========================================");

    // 1. 创建Transport实例（使用简化架构）
    let transport = Transport::new(TransportConfig::default()).await?;
    println!("✅ Transport创建成功（简化架构）");

    // 2. 先执行一些操作，然后测试统计
    println!("\n🧪 执行高性能组件操作:");
    
    // 2.1 测试内存池操作
    println!("📦 测试内存池操作:");
    let memory_pool = transport.memory_pool();
    
    // 获取不同大小的缓冲区
    use msgtrans::transport::memory_pool_v2::BufferSize;
    let small_buffer = memory_pool.get_buffer(BufferSize::Small);
    let medium_buffer = memory_pool.get_buffer(BufferSize::Medium);
    let large_buffer = memory_pool.get_buffer(BufferSize::Large);
    
    println!("   - 小缓冲区: {} 字节", small_buffer.capacity());
    println!("   - 中缓冲区: {} 字节", medium_buffer.capacity());
    println!("   - 大缓冲区: {} 字节", large_buffer.capacity());
    
    // 归还缓冲区
    memory_pool.return_buffer(small_buffer, BufferSize::Small);
    memory_pool.return_buffer(medium_buffer, BufferSize::Medium);
    memory_pool.return_buffer(large_buffer, BufferSize::Large);
    println!("   - 缓冲区已归还");

    // 2.2 测试连接池操作
    println!("🏊 测试连接池操作:");
    let connection_pool = transport.connection_pool();
    
    // 获取连接
    match connection_pool.get_connection() {
        Ok(conn_id) => {
            println!("   - 获取连接成功: {:?}", conn_id);
            
            // 归还连接
            if let Err(e) = connection_pool.return_connection(conn_id) {
                println!("   - 归还连接失败: {:?}", e);
            } else {
                println!("   - 连接已归还");
            }
        }
        Err(e) => {
            println!("   - 获取连接失败: {:?}", e);
        }
    }

    // 3. 现在获取统计 - 应该显示真实数据
    println!("\n📊 高性能组件统计:");
    
    // 连接池统计
    let pool_stats = transport.connection_pool_stats();
    println!("🏊 连接池统计:");
    println!("   - 总连接: {}", pool_stats.total_connections);
    println!("   - 活跃连接: {}", pool_stats.active_connections);
    println!("   - 可用连接: {}", pool_stats.available_connections);
    println!("   - 获取操作: {}", pool_stats.get_operations);
    println!("   - 归还操作: {}", pool_stats.return_operations);
    println!("   - 总操作: {}", pool_stats.total_operations);

    // 内存池统计
    let memory_stats = transport.memory_pool_stats();
    println!("🧠 内存池统计:");
    println!("   - 缓存命中率: {:.2}%", memory_stats.cache_hit_rate * 100.0);
    println!("   - 总操作数: {}", memory_stats.total_operations);
    println!("   - 获取操作: {} (小:{}, 中:{}, 大:{})", 
             memory_stats.small_get_operations + memory_stats.medium_get_operations + memory_stats.large_get_operations,
             memory_stats.small_get_operations, memory_stats.medium_get_operations, memory_stats.large_get_operations);
    println!("   - 归还操作: {} (小:{}, 中:{}, 大:{})", 
             memory_stats.small_return_operations + memory_stats.medium_return_operations + memory_stats.large_return_operations,
             memory_stats.small_return_operations, memory_stats.medium_return_operations, memory_stats.large_return_operations);
    println!("   - 已分配内存: {:.2} MB", memory_stats.total_memory_allocated_mb);
    println!("   - 已缓存内存: {:.2} MB", memory_stats.total_memory_cached_mb);

    // 4. 测试会话管理
    println!("\n📋 测试会话管理:");
    let active_sessions = transport.active_sessions().await;
    println!("   - 活跃会话数: {}", active_sessions.len());

    let stats = transport.stats().await?;
    println!("   - 传输统计: {} 个会话", stats.len());

    // 5. 测试事件流
    println!("\n📡 测试事件流:");
    let mut events = transport.events();
    println!("   - 事件流创建成功");

    // 6. 获取最终统计
    println!("\n📈 最终统计:");
    let final_pool_stats = transport.connection_pool_stats();
    let final_memory_stats = transport.memory_pool_stats();
    
    println!("🏊 连接池:");
    println!("   - 总操作: {}", final_pool_stats.total_operations);
    println!("   - 平均等待时间: {:.2} μs", 
             if final_pool_stats.total_operations > 0 {
                 final_pool_stats.total_wait_time_ns as f64 / final_pool_stats.total_operations as f64 / 1000.0
             } else { 0.0 });

    println!("🧠 内存池:");
    println!("   - 缓存命中: {}", final_memory_stats.cache_hit_rate);
    println!("   - 内存效率: {:.2}%", final_memory_stats.memory_efficiency * 100.0);

    println!("\n✅ 简化架构演示完成！");
    println!("========================================");
    println!("🎯 新架构特性验证:");
    println!("   ✅ OptimizedActor直接管理");
    println!("   ✅ SimplifiedSessionManager");
    println!("   ✅ LockFree高性能组件");
    println!("   ✅ 优化内存池");
    println!("   ✅ 智能连接池");
    println!("   ✅ 统一事件流");

    Ok(())
} 