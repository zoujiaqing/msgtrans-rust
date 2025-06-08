/// Phase 2 演示 - 智能扩展机制
/// 
/// 这个示例展示了Phase 2的核心功能：
/// 1. 渐进式资源扩展算法
/// 2. 零拷贝内存池管理
/// 3. 实时性能监控
/// 4. 智能负载适应

use msgtrans::{
    transport::{ConnectionPool, ExpansionStrategy, MemoryPool, BufferSize, PerformanceMetrics, PoolDetailedStatus, MemoryPoolStatus},
    TransportError
};
use std::time::Duration;
use tokio::time::sleep;
use bytes::BytesMut;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 msgtrans Phase 2 演示 - 智能扩展机制");
    
    // 1. 演示智能连接池扩展
    demo_smart_connection_pool().await?;
    
    // 2. 演示零拷贝内存池
    demo_memory_pool().await?;
    
    // 3. 演示性能监控
    demo_performance_monitoring().await?;
    
    // 4. 演示压力测试场景
    demo_stress_test_scenario().await?;
    
    println!("✅ Phase 2 演示完成");
    Ok(())
}

/// 演示智能连接池扩展
async fn demo_smart_connection_pool() -> Result<(), TransportError> {
    println!("\n📈 1. 智能连接池扩展演示");
    
    // 创建智能连接池：初始100，最大8000
    let mut pool = ConnectionPool::new(100, 8000);
    
    println!("初始状态:");
    let status = pool.detailed_status().await;
    print_pool_status(&status);
    
    // 模拟负载增加，触发扩展
    println!("\n模拟负载增加...");
    
    // 第一次扩展 (2.0x: 100 -> 200)
    pool.force_expand().await?;
    let status = pool.detailed_status().await;
    println!("第一次扩展后:");
    print_pool_status(&status);
    
    // 第二次扩展 (2.0x: 200 -> 400) 
    pool.force_expand().await?;
    let status = pool.detailed_status().await;
    println!("第二次扩展后:");
    print_pool_status(&status);
    
    // 第三次扩展 (2.0x: 400 -> 800)
    pool.force_expand().await?;
    let status = pool.detailed_status().await;
    println!("第三次扩展后:");
    print_pool_status(&status);
    
    // 第四次扩展 (1.5x: 800 -> 1200)
    pool.force_expand().await?;
    let status = pool.detailed_status().await;
    println!("第四次扩展后 (切换到1.5x因子):");
    print_pool_status(&status);
    
    // 演示收缩
    println!("\n模拟负载降低...");
    pool.try_shrink().await?;
    let status = pool.detailed_status().await;
    println!("收缩后:");
    print_pool_status(&status);
    
    Ok(())
}

/// 演示零拷贝内存池
async fn demo_memory_pool() -> Result<(), TransportError> {
    println!("\n🧠 2. 零拷贝内存池演示");
    
    let memory_pool = MemoryPool::new();
    
    println!("初始内存池状态:");
    let status = memory_pool.status().await;
    print_memory_status(&status);
    
    // 分配不同大小的缓冲区
    println!("\n分配缓冲区...");
    
    let mut buffers = Vec::new();
    
    // 分配小缓冲区
    for i in 0..10 {
        let buffer = memory_pool.get_buffer(BufferSize::Small).await?;
        println!("分配小缓冲区 #{}: 容量 {}KB", i+1, buffer.capacity() / 1024);
        buffers.push((buffer, BufferSize::Small));
    }
    
    // 分配中缓冲区
    for i in 0..5 {
        let buffer = memory_pool.get_buffer(BufferSize::Medium).await?;
        println!("分配中缓冲区 #{}: 容量 {}KB", i+1, buffer.capacity() / 1024);
        buffers.push((buffer, BufferSize::Medium));
    }
    
    // 分配大缓冲区
    for i in 0..3 {
        let buffer = memory_pool.get_buffer(BufferSize::Large).await?;
        println!("分配大缓冲区 #{}: 容量 {}KB", i+1, buffer.capacity() / 1024);
        buffers.push((buffer, BufferSize::Large));
    }
    
    println!("\n分配后内存池状态:");
    let status = memory_pool.status().await;
    print_memory_status(&status);
    
    // 归还缓冲区
    println!("\n归还缓冲区...");
    for (buffer, size) in buffers {
        memory_pool.return_buffer(buffer, size).await;
    }
    
    println!("归还后内存池状态:");
    let status = memory_pool.status().await;
    print_memory_status(&status);
    
    Ok(())
}

/// 演示性能监控
async fn demo_performance_monitoring() -> Result<(), TransportError> {
    println!("\n📊 3. 性能监控演示");
    
    let mut pool = ConnectionPool::new(100, 2000);
    
    // 连续执行多次扩展和收缩操作
    println!("执行连续的扩展/收缩操作...");
    
    for i in 1..=5 {
        pool.force_expand().await?;
        sleep(Duration::from_millis(100)).await;
        
        if i % 2 == 0 {
            pool.try_shrink().await?;
            sleep(Duration::from_millis(100)).await;
        }
    }
    
    // 获取性能指标
    let status = pool.detailed_status().await;
    println!("\n性能指标总览:");
    println!("  扩展次数: {}", status.expansion_count);
    println!("  收缩次数: {}", status.shrink_count);
    println!("  当前扩展因子: {:.1}x", status.current_expansion_factor);
    println!("  平均使用率: {:.1}%", status.avg_utilization * 100.0);
    println!("  内存使用: {:.2}MB", status.memory_pool_status.total_memory_mb);
    
    Ok(())
}

/// 演示压力测试场景
async fn demo_stress_test_scenario() -> Result<(), TransportError> {
    println!("\n🔥 4. 压力测试场景演示");
    
    // 模拟高并发场景
    let mut pool = ConnectionPool::new(50, 5000);
    let memory_pool = pool.memory_pool();
    
    println!("模拟高并发请求突增...");
    
    // 并发分配大量缓冲区
    let mut handles = Vec::new();
    
    for i in 0..100 {
        let memory_pool_clone = memory_pool.clone();
        let handle = tokio::spawn(async move {
            // 随机选择缓冲区大小
            let size = match i % 3 {
                0 => BufferSize::Small,
                1 => BufferSize::Medium,
                _ => BufferSize::Large,
            };
            
            match memory_pool_clone.get_buffer(size).await {
                Ok(buffer) => {
                    // 模拟使用缓冲区
                    sleep(Duration::from_millis(10)).await;
                    memory_pool_clone.return_buffer(buffer, size).await;
                    Ok(())
                },
                Err(e) => Err(e),
            }
        });
        handles.push(handle);
    }
    
    // 等待所有任务完成
    let mut success_count = 0;
    let mut error_count = 0;
    
    for handle in handles {
        match handle.await {
            Ok(Ok(())) => success_count += 1,
            Ok(Err(_)) => error_count += 1,
            Err(_) => error_count += 1,
        }
    }
    
    println!("压力测试结果:");
    println!("  成功: {}", success_count);
    println!("  失败: {}", error_count);
    println!("  成功率: {:.1}%", success_count as f64 / (success_count + error_count) as f64 * 100.0);
    
    // 最终状态
    let status = pool.detailed_status().await;
    println!("\n最终池状态:");
    print_pool_status(&status);
    
    Ok(())
}

/// 打印连接池状态
fn print_pool_status(status: &PoolDetailedStatus) {
    println!("  当前大小: {} / {}", status.current_size, status.max_size);
    println!("  使用率: {:.1}%", status.utilization * 100.0);
    println!("  扩展因子: {:.1}x", status.current_expansion_factor);
    println!("  扩展次数: {}", status.expansion_count);
    println!("  收缩次数: {}", status.shrink_count);
}

/// 打印内存池状态  
fn print_memory_status(status: &MemoryPoolStatus) {
    println!("  小缓冲区池: {} 个", status.small_pool_size);
    println!("  中缓冲区池: {} 个", status.medium_pool_size);
    println!("  大缓冲区池: {} 个", status.large_pool_size);
    println!("  已分配: 小={}, 中={}, 大={}", 
             status.small_allocated, 
             status.medium_allocated, 
             status.large_allocated);
    println!("  总内存: {:.2}MB", status.total_memory_mb);
} 