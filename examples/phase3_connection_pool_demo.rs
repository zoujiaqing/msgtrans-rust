/// Phase 3.1.1 连接池优化演示
/// 
/// 验证 LockFree + Crossbeam + Tokio 混合架构的性能提升：
/// - LockFree 连接存储：wait-free 连接获取和归还
/// - Crossbeam 控制通道：高性能同步操作
/// - Tokio 事件广播：生态集成和监控

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use msgtrans::transport::pool::{ConnectionPool, ConnectionId, PoolEvent};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();
    
    println!("🚀 Phase 3.1.1: 连接池优化演示");
    println!("===============================");
    
    // 创建优化后的连接池
    let mut pool = ConnectionPool::new(50, 200)
        .initialize_pool()
        .await?;
    
    // 创建事件监听器
    let mut event_receiver = pool.event_broadcaster.subscribe();
    
    // 启动事件监听任务
    let event_task = tokio::spawn(async move {
        let mut event_count = 0;
        while let Ok(event) = event_receiver.recv().await {
            event_count += 1;
            match event {
                PoolEvent::ConnectionCreated { connection_id } => {
                    if event_count <= 10 {
                        println!("📡 事件: 连接创建 {:?}", connection_id);
                    }
                },
                PoolEvent::ConnectionAcquired { connection_id } => {
                    if event_count <= 20 {
                        println!("📡 事件: 连接获取 {:?}", connection_id);
                    }
                },
                PoolEvent::ConnectionReleased { connection_id } => {
                    if event_count <= 20 {
                        println!("📡 事件: 连接归还 {:?}", connection_id);
                    }
                },
                PoolEvent::PoolExpanded { from_size, to_size } => {
                    println!("📡 事件: 连接池扩展 {} -> {}", from_size, to_size);
                },
                PoolEvent::PoolShrunk { from_size, to_size } => {
                    println!("📡 事件: 连接池收缩 {} -> {}", from_size, to_size);
                },
                _ => {}
            }
            
            if event_count >= 100 {
                break;
            }
        }
        println!("📡 事件监听结束，处理了 {} 个事件", event_count);
    });
    
    // Phase 3.1.1 性能测试
    println!("\n🚀 Phase 3.1.1 性能测试开始...");
    
    // 测试1: 基础连接操作性能
    println!("\n📊 测试1: 基础连接操作性能");
    test_basic_operations(&pool).await;
    
    // 测试2: 并发连接操作性能
    println!("\n📊 测试2: 并发连接操作性能");
    test_concurrent_operations(&pool).await;
    
    // 测试3: 连接池智能扩展
    println!("\n📊 测试3: 连接池智能扩展");
    test_smart_expansion(&mut pool).await;
    
    // 测试4: 连接池智能收缩
    println!("\n📊 测试4: 连接池智能收缩");
    test_smart_shrinking(&mut pool).await;
    
    // 显示最终统计
    println!("\n📈 最终性能统计:");
    let final_stats = pool.get_performance_stats();
    print_performance_stats(&final_stats);
    
    // 等待事件任务完成
    sleep(Duration::from_millis(100)).await;
    drop(pool); // 触发事件任务结束
    let _ = event_task.await;
    
    println!("\n🎉 Phase 3.1.1 连接池优化演示完成！");
    
    Ok(())
}

/// 测试基础连接操作性能
async fn test_basic_operations(pool: &ConnectionPool) {
    let iterations = 1000;
    let start_time = Instant::now();
    
    println!("   🔧 执行 {} 次连接获取/归还操作...", iterations);
    
    for i in 0..iterations {
        // 获取连接
        match pool.get_connection() {
            Ok(connection_id) => {
                // 模拟使用连接
                if i % 100 == 0 {
                    tokio::task::yield_now().await;
                }
                
                // 归还连接
                if let Err(e) = pool.return_connection(connection_id) {
                    eprintln!("❌ 归还连接失败: {:?}", e);
                    break;
                }
            },
            Err(e) => {
                eprintln!("❌ 获取连接失败: {:?}", e);
                break;
            }
        }
    }
    
    let duration = start_time.elapsed();
    let ops_per_sec = (iterations as f64 * 2.0) / duration.as_secs_f64(); // 获取+归还=2个操作
    
    println!("   ⚡ 基础操作性能: {:.0} ops/s ({} 操作耗时 {:?})", 
             ops_per_sec, iterations * 2, duration);
    
    // 显示当前统计
    let stats = pool.get_performance_stats();
    println!("   📊 操作统计: 获取={}, 归还={}, 平均等待时间={:.2}μs",
             stats.get_operations, 
             stats.return_operations,
             if stats.total_operations > 0 {
                 (stats.total_wait_time_ns as f64 / stats.total_operations as f64) / 1000.0
             } else { 0.0 });
}

/// 测试并发连接操作性能
async fn test_concurrent_operations(pool: &ConnectionPool) {
    let num_tasks = 10;
    let operations_per_task = 200;
    
    println!("   🔧 启动 {} 个并发任务，每个执行 {} 次操作...", num_tasks, operations_per_task);
    
    let start_time = Instant::now();
    let mut tasks = Vec::new();
    
    for task_id in 0..num_tasks {
        let pool_ref = pool.clone();
        
        let task = tokio::spawn(async move {
            let mut successful_ops = 0;
            
            for _ in 0..operations_per_task {
                match pool_ref.get_connection() {
                    Ok(connection_id) => {
                        // 模拟短暂使用
                        tokio::task::yield_now().await;
                        
                        if pool_ref.return_connection(connection_id).is_ok() {
                            successful_ops += 1;
                        }
                    },
                    Err(_) => {
                        // 连接池可能暂时耗尽，稍等重试
                        tokio::time::sleep(Duration::from_micros(100)).await;
                    }
                }
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
    
    // 显示利用率
    println!("   📊 当前连接池利用率: {:.1}%", pool.utilization() * 100.0);
}

/// 测试智能扩展
async fn test_smart_expansion(pool: &mut ConnectionPool) {
    println!("   🔧 测试智能扩展机制...");
    
    let initial_stats = pool.get_performance_stats();
    println!("   📊 扩展前: 总连接={}, 可用连接={}", 
             initial_stats.total_connections, 
             initial_stats.available_connections);
    
    // 模拟高负载，耗尽大部分连接
    let mut held_connections = Vec::new();
    for _ in 0..45 { // 获取45个连接，使利用率达到90%
        if let Ok(connection_id) = pool.get_connection() {
            held_connections.push(connection_id);
        }
    }
    
    println!("   📊 高负载后利用率: {:.1}%", pool.utilization() * 100.0);
    
    // 尝试智能扩展
    match pool.try_expand().await {
        Ok(true) => {
            println!("   ✅ 智能扩展成功!");
            let expanded_stats = pool.get_performance_stats();
            println!("   📊 扩展后: 总连接={}, 可用连接={}", 
                     expanded_stats.total_connections, 
                     expanded_stats.available_connections);
        },
        Ok(false) => {
            println!("   ℹ️ 不需要扩展 (利用率未达到阈值)");
        },
        Err(e) => {
            println!("   ❌ 扩展失败: {:?}", e);
        }
    }
    
    // 归还连接
    for connection_id in held_connections {
        let _ = pool.return_connection(connection_id);
    }
}

/// 测试智能收缩
async fn test_smart_shrinking(pool: &mut ConnectionPool) {
    println!("   🔧 测试智能收缩机制...");
    
    let initial_stats = pool.get_performance_stats();
    println!("   📊 收缩前: 总连接={}, 可用连接={}, 利用率={:.1}%", 
             initial_stats.total_connections, 
             initial_stats.available_connections,
             pool.utilization() * 100.0);
    
    // 尝试智能收缩
    match pool.try_shrink().await {
        Ok(true) => {
            println!("   ✅ 智能收缩成功!");
            let shrunk_stats = pool.get_performance_stats();
            println!("   📊 收缩后: 总连接={}, 可用连接={}", 
                     shrunk_stats.total_connections, 
                     shrunk_stats.available_connections);
        },
        Ok(false) => {
            println!("   ℹ️ 不需要收缩 (利用率高于阈值或已达最小值)");
        },
        Err(e) => {
            println!("   ❌ 收缩失败: {:?}", e);
        }
    }
}

/// 打印性能统计
fn print_performance_stats(stats: &msgtrans::transport::pool::OptimizedPoolStatsSnapshot) {
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("   📊 LockFree 连接池统计:");
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("   🔗 连接统计:");
    println!("      总连接数: {}", stats.total_connections);
    println!("      活跃连接: {}", stats.active_connections);
    println!("      可用连接: {}", stats.available_connections);
    
    println!("   ⚡ 操作统计:");
    println!("      获取操作: {}", stats.get_operations);
    println!("      归还操作: {}", stats.return_operations);
    println!("      创建操作: {}", stats.create_operations);
    println!("      移除操作: {}", stats.remove_operations);
    println!("      总操作数: {}", stats.total_operations);
    
    println!("   ⏱️  性能统计:");
    let avg_wait_time_us = if stats.total_operations > 0 {
        (stats.total_wait_time_ns as f64 / stats.total_operations as f64) / 1000.0
    } else {
        0.0
    };
    println!("      平均等待时间: {:.2} μs", avg_wait_time_us);
    println!("      总等待时间: {:.2} ms", stats.total_wait_time_ns as f64 / 1_000_000.0);
    
    if stats.total_operations > 0 {
        let ops_per_ms = stats.total_operations as f64 / (stats.total_wait_time_ns as f64 / 1_000_000.0);
        println!("      估算吞吐量: {:.0} ops/ms", ops_per_ms);
    }
    println!("   ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
} 