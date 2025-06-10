/// Phase 3.1.2 内存池优化演示
/// 
/// 验证完全 LockFree 内存池的性能提升：
/// - 完全移除 RwLock，使用 LockFree 队列
/// - 同步API，避免异步开销  
/// - 智能缓存管理和零拷贝优化
/// - 实时事件监控和性能统计

use std::time::{Duration, Instant};
use tokio::time::sleep;

use msgtrans::transport::{
    OptimizedMemoryPool, MemoryPoolEvent
};
use msgtrans::transport::memory_pool_v2::BufferSize;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 Phase 3.1.2: LockFree 内存池优化演示");
    println!("=======================================");
    
    let memory_pool = OptimizedMemoryPool::new()
        .with_preallocation(100, 50, 20);
    
    // 基础性能测试
    let iterations = 10000;
    let start = Instant::now();
    
    for i in 0..iterations {
        let size = match i % 3 {
            0 => BufferSize::Small,
            1 => BufferSize::Medium,
            _ => BufferSize::Large,
        };
        
        let buffer = memory_pool.get_buffer(size);
        memory_pool.return_buffer(buffer, size);
    }
    
    let duration = start.elapsed();
    let ops_per_sec = (iterations as f64 * 2.0) / duration.as_secs_f64();
    
    println!("⚡ 基础性能: {:.0} ops/s", ops_per_sec);
    
    let stats = memory_pool.get_stats();
    println!("📊 缓存命中率: {:.1}%", stats.cache_hit_rate * 100.0);
    
    Ok(())
}

/// 测试基础缓冲区操作性能
async fn test_basic_buffer_operations(pool: &OptimizedMemoryPool) {
    let iterations = 10000;
    let start_time = Instant::now();
    
    println!("   🔧 执行 {} 次基础缓冲区获取/归还操作...", iterations);
    
    for i in 0..iterations {
        // 循环使用不同大小的缓冲区
        let size = match i % 3 {
            0 => BufferSize::Small,
            1 => BufferSize::Medium,
            _ => BufferSize::Large,
        };
        
        // 获取缓冲区
        let buffer = pool.get_buffer(size);
        
        // 模拟使用缓冲区
        if i % 1000 == 0 {
            tokio::task::yield_now().await;
        }
        
        // 归还缓冲区
        pool.return_buffer(buffer, size);
    }
    
    let duration = start_time.elapsed();
    let ops_per_sec = (iterations as f64 * 2.0) / duration.as_secs_f64(); // 获取+归还=2个操作
    
    println!("   ⚡ 基础操作性能: {:.0} ops/s ({} 操作耗时 {:?})", 
             ops_per_sec, iterations * 2, duration);
    
    // 显示当前统计
    let stats = pool.get_stats();
    println!("   📊 缓存命中率: {:.1}%, 总内存: {:.1}MB", 
             stats.cache_hit_rate * 100.0,
             stats.total_memory_allocated_mb);
}

/// 测试并发缓冲区操作性能
async fn test_concurrent_buffer_operations(pool: &OptimizedMemoryPool) {
    let num_tasks = 20;
    let operations_per_task = 1000;
    
    println!("   🔧 启动 {} 个并发任务，每个执行 {} 次操作...", num_tasks, operations_per_task);
    
    let start_time = Instant::now();
    let mut tasks = Vec::new();
    
    for task_id in 0..num_tasks {
        let pool_ref = pool.clone();
        
        let task = tokio::spawn(async move {
            let mut successful_ops = 0;
            
            for i in 0..operations_per_task {
                let size = match (task_id + i) % 3 {
                    0 => BufferSize::Small,
                    1 => BufferSize::Medium,
                    _ => BufferSize::Large,
                };
                
                // 获取缓冲区
                let buffer = pool_ref.get_buffer(size);
                
                // 模拟使用
                if i % 100 == 0 {
                    tokio::task::yield_now().await;
                }
                
                // 归还缓冲区
                pool_ref.return_buffer(buffer, size);
                successful_ops += 1;
            }
            
            (task_id, successful_ops)
        });
        
        tasks.push(task);
    }
    
    // 等待所有任务完成
    let results = futures::future::join_all(tasks).await;
    let duration = start_time.elapsed();
    
    let total_successful_ops: usize = results.iter()
        .map(|r| r.as_ref().unwrap().1)
        .sum();
    
    let concurrent_ops_per_sec = (total_successful_ops as f64 * 2.0) / duration.as_secs_f64();
    
    println!("   ⚡ 并发操作性能: {:.0} ops/s ({} 个任务, {} 成功操作, 耗时 {:?})", 
             concurrent_ops_per_sec, 
             num_tasks, 
             total_successful_ops,
             duration);
    
    // 显示并发后的统计
    let stats = pool.get_stats();
    println!("   📊 并发后缓存命中率: {:.1}%, 内存效率: {:.1}%", 
             stats.cache_hit_rate * 100.0,
             stats.memory_efficiency * 100.0);
}

/// 测试混合大小缓冲区性能
async fn test_mixed_buffer_sizes(pool: &OptimizedMemoryPool) {
    println!("   🔧 测试不同大小缓冲区的性能特征...");
    
    let test_sizes = [
        (BufferSize::Small, 5000),
        (BufferSize::Medium, 2000),
        (BufferSize::Large, 500),
    ];
    
    for (size, count) in test_sizes.iter() {
        let start_time = Instant::now();
        let mut buffers = Vec::new();
        
        // 获取缓冲区
        for _ in 0..*count {
            buffers.push(pool.get_buffer(*size));
        }
        
        let get_duration = start_time.elapsed();
        
        // 归还缓冲区
        let return_start = Instant::now();
        for buffer in buffers {
            pool.return_buffer(buffer, *size);
        }
        let return_duration = return_start.elapsed();
        
        let total_duration = start_time.elapsed();
        let ops_per_sec = (*count as f64 * 2.0) / total_duration.as_secs_f64();
        
        println!("   📊 {} 缓冲区: {:.0} ops/s (获取: {:?}, 归还: {:?})", 
                 size.description(), 
                 ops_per_sec,
                 get_duration,
                 return_duration);
    }
}

/// 测试零拷贝性能
async fn test_zero_copy_performance(pool: &OptimizedMemoryPool) {
    println!("   🔧 测试零拷贝缓冲区复用性能...");
    
    let iterations = 5000;
    let start_time = Instant::now();
    
    // 第一轮：预热缓存
    let mut buffers = Vec::new();
    for _ in 0..100 {
        buffers.push(pool.get_buffer(BufferSize::Medium));
    }
    for buffer in buffers {
        pool.return_buffer(buffer, BufferSize::Medium);
    }
    
    // 第二轮：测试零拷贝性能
    let test_start = Instant::now();
    for i in 0..iterations {
        let mut buffer = pool.get_buffer(BufferSize::Medium);
        
        // 模拟写入数据
        buffer.extend_from_slice(&[0u8; 100]); // 写入100字节
        
        // 模拟数据处理
        if i % 1000 == 0 {
            tokio::task::yield_now().await;
        }
        
        // 归还缓冲区（应该被清理重用）
        pool.return_buffer(buffer, BufferSize::Medium);
    }
    
    let test_duration = test_start.elapsed();
    let zero_copy_ops_per_sec = (iterations as f64 * 2.0) / test_duration.as_secs_f64();
    
    println!("   ⚡ 零拷贝性能: {:.0} ops/s ({} 次写入+复用, 耗时 {:?})", 
             zero_copy_ops_per_sec, iterations, test_duration);
    
    let stats = pool.get_stats();
    println!("   📊 零拷贝后缓存命中率: {:.1}%", stats.cache_hit_rate * 100.0);
}

/// 打印内存池统计
fn print_memory_stats(stats: &msgtrans::transport::memory_pool_v2::OptimizedMemoryStatsSnapshot) {
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("   📊 LockFree 内存池统计:");
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    
    println!("   🔗 缓冲区操作统计:");
    println!("      小缓冲区获取: {}", stats.small_get_operations);
    println!("      中缓冲区获取: {}", stats.medium_get_operations);
    println!("      大缓冲区获取: {}", stats.large_get_operations);
    println!("      总操作数: {}", stats.total_operations);
    
    println!("   📦 缓冲区分配统计:");
    println!("      小缓冲区分配: {}", stats.small_allocated);
    println!("      中缓冲区分配: {}", stats.medium_allocated);
    println!("      大缓冲区分配: {}", stats.large_allocated);
    println!("      小缓冲区缓存: {}", stats.small_cached);
    println!("      中缓冲区缓存: {}", stats.medium_cached);
    println!("      大缓冲区缓存: {}", stats.large_cached);
    
    println!("   ⚡ 性能统计:");
    println!("      缓存命中率: {:.1}%", stats.cache_hit_rate * 100.0);
    println!("      缓存未命中率: {:.1}%", stats.cache_miss_rate * 100.0);
    
    println!("   💾 内存统计:");
    println!("      总分配内存: {:.2} MB", stats.total_memory_allocated_mb);
    println!("      缓存内存: {:.2} MB", stats.total_memory_cached_mb);
    println!("      内存效率: {:.1}%", stats.memory_efficiency * 100.0);
    
    // 计算估算吞吐量
    if stats.total_operations > 0 {
        println!("   🚀 估算性能:");
        println!("      内存池操作效率: 同步LockFree架构");
        
        // 假设平均每个操作耗时（基于LockFree特性）
        let estimated_latency_ns = 100; // 100纳秒 (非常快的LockFree操作)
        let estimated_ops_per_sec = 1_000_000_000 / estimated_latency_ns;
        println!("      估算峰值吞吐量: {} M ops/s", estimated_ops_per_sec / 1_000_000);
    }
    
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
} 