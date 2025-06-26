/// 简单的性能基准测试
/// 重点展示无锁连接的核心改进点

use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};

#[tokio::main]
async fn main() {
    println!("🎯 msgtrans 无锁连接性能改进演示");
    println!("==============================");
    
    // 测试1: 锁竞争对比
    test_lock_contention().await;
    
    // 测试2: 原子操作性能
    test_atomic_performance().await;
    
    // 测试3: 事件处理阻塞问题
    test_event_processing_blocking().await;
    
    println!("\n🎉 性能改进总结");
    println!("===============");
    println!("1. 🔒 锁竞争消除: 使用原子操作替代 Mutex 锁");
    println!("2. 📦 消息队列化: 避免直接阻塞调用");
    println!("3. 🎭 事件隔离: 阻塞操作不影响事件循环");
    println!("4. 💾 内存效率: 减少内存分配和锁开销");
}

/// 测试锁竞争影响
async fn test_lock_contention() {
    println!("\n🔍 测试1: 锁竞争性能对比");
    
    let iterations = 100_000;
    let concurrency = 10;
    
    // 传统方式：使用 Mutex
    let counter_mutex = Arc::new(Mutex::new(0u64));
    let start = Instant::now();
    
    let mut handles = Vec::new();
    for _ in 0..concurrency {
        let counter = counter_mutex.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..iterations / concurrency {
                {
                    let mut guard = counter.lock().unwrap();
                    *guard += 1;
                } // 锁在这里被释放
                // 模拟一些计算工作
                tokio::task::yield_now().await;
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    let mutex_time = start.elapsed();
    let mutex_count = *counter_mutex.lock().unwrap();
    
    // 无锁方式：使用原子操作
    let counter_atomic = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    
    let mut handles = Vec::new();
    for _ in 0..concurrency {
        let counter = counter_atomic.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..iterations / concurrency {
                counter.fetch_add(1, Ordering::Relaxed);
                // 模拟一些计算工作
                tokio::task::yield_now().await;
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    let atomic_time = start.elapsed();
    let atomic_count = counter_atomic.load(Ordering::Relaxed);
    
    println!("   Mutex 方式: {:?} (计数: {})", mutex_time, mutex_count);
    println!("   原子操作: {:?} (计数: {})", atomic_time, atomic_count);
    println!("   🚀 性能提升: {:.2}x", mutex_time.as_secs_f64() / atomic_time.as_secs_f64());
}

/// 测试原子操作性能
async fn test_atomic_performance() {
    println!("\n🔍 测试2: 原子操作vs锁性能");
    
    let operations = 1_000_000;
    
    // Mutex 计数器
    let mutex_counter = Arc::new(Mutex::new(0u64));
    let start = Instant::now();
    for _ in 0..operations {
        let mut guard = mutex_counter.lock().unwrap();
        *guard += 1;
    }
    let mutex_time = start.elapsed();
    
    // 原子计数器
    let atomic_counter = Arc::new(AtomicU64::new(0));
    let start = Instant::now();
    for _ in 0..operations {
        atomic_counter.fetch_add(1, Ordering::Relaxed);
    }
    let atomic_time = start.elapsed();
    
    println!("   Mutex 操作: {:?} ({} ops)", mutex_time, operations);
    println!("   原子操作: {:?} ({} ops)", atomic_time, operations);
    
    let mutex_ops_per_sec = operations as f64 / mutex_time.as_secs_f64();
    let atomic_ops_per_sec = operations as f64 / atomic_time.as_secs_f64();
    
    println!("   Mutex OPS: {:.0}/秒", mutex_ops_per_sec);
    println!("   原子 OPS: {:.0}/秒", atomic_ops_per_sec);
    println!("   🚀 性能提升: {:.2}x", atomic_ops_per_sec / mutex_ops_per_sec);
}

/// 测试事件处理阻塞问题
async fn test_event_processing_blocking() {
    println!("\n🔍 测试3: 事件处理阻塞问题演示");
    
    // 模拟阻塞的事件处理
    println!("   ❌ 错误方式: 事件循环中有阻塞操作");
    let start = Instant::now();
    
    for i in 0..5 {
        println!("      处理事件 #{}", i + 1);
        // 模拟阻塞操作（如网络发送）
        tokio::time::sleep(Duration::from_millis(100)).await;
        // 在实际场景中，其他事件会在这里等待
    }
    
    let blocking_time = start.elapsed();
    println!("   总时间: {:?}", blocking_time);
    
    // 非阻塞的事件处理
    println!("   ✅ 正确方式: 阻塞操作隔离到单独任务");
    let start = Instant::now();
    
    let mut handles = Vec::new();
    for i in 0..5 {
        println!("      处理事件 #{}", i + 1);
        // 将阻塞操作移到单独的任务中
        let handle = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            println!("        事件 #{} 处理完成", i + 1);
        });
        handles.push(handle);
    }
    
    // 事件循环立即继续，不等待阻塞操作
    let event_loop_time = start.elapsed();
    println!("   事件循环时间: {:?}", event_loop_time);
    
    // 等待所有后台任务完成
    for handle in handles {
        handle.await.unwrap();
    }
    
    let total_time = start.elapsed();
    println!("   总时间: {:?}", total_time);
    println!("   🚀 事件循环响应提升: {:.2}x", blocking_time.as_secs_f64() / event_loop_time.as_secs_f64());
    println!("   📊 并发执行总体提升: {:.2}x", blocking_time.as_secs_f64() / total_time.as_secs_f64());
} 