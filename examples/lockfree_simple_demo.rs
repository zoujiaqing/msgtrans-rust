/// 第一阶段无锁优化简单演示
/// 
/// 展示无锁HashMap、队列、计数器的基本功能和性能

use msgtrans::transport::lockfree_enhanced::{LockFreeHashMap, LockFreeQueue, LockFreeCounter};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn main() {
    println!("🚀 第一阶段无锁优化演示");
    println!("=======================");
    
    // 1. 无锁HashMap演示
    demo_lockfree_hashmap();
    
    // 2. 无锁队列演示
    demo_lockfree_queue();
    
    // 3. 无锁计数器演示
    demo_lockfree_counter();
    
    // 4. 并发性能测试
    demo_concurrent_performance();
    
    println!("\n✅ 所有演示完成！");
}

/// 演示无锁HashMap
fn demo_lockfree_hashmap() {
    println!("\n📊 1. 无锁HashMap演示");
    
    let map = Arc::new(LockFreeHashMap::new());
    
    // 基本操作演示
    println!("基本操作:");
    map.insert("key1".to_string(), "value1".to_string()).unwrap();
    map.insert("key2".to_string(), "value2".to_string()).unwrap();
    
    println!("  - 插入 key1 -> value1");
    println!("  - 插入 key2 -> value2");
    
    if let Some(value) = map.get(&"key1".to_string()) {
        println!("  - 读取 key1: {}", value);
    }
    
    println!("  - 当前大小: {}", map.len());
    
    // 并发操作演示
    println!("并发操作测试:");
    let map_clone = Arc::clone(&map);
    let mut handles = vec![];
    
    let start = Instant::now();
    
    // 启动10个线程并发写入
    for i in 0..10 {
        let map = Arc::clone(&map_clone);
        let handle = thread::spawn(move || {
            for j in 0..100 {
                let key = format!("thread_{}_key_{}", i, j);
                let value = format!("thread_{}_value_{}", i, j);
                let _ = map.insert(key, value);
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    let stats = map.stats();
    
    println!("  - 10个线程，每个写入100次");
    println!("  - 总耗时: {:?}", duration);
    println!("  - 最终大小: {}", map.len());
    println!("  - 读取次数: {}", stats.reads.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - 写入次数: {}", stats.writes.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - CAS成功率: {:.2}%", stats.cas_success_rate() * 100.0);
}

/// 演示无锁队列
fn demo_lockfree_queue() {
    println!("\n🔄 2. 无锁队列演示");
    
    let queue = Arc::new(LockFreeQueue::new());
    
    // 基本操作演示
    println!("基本操作:");
    queue.push(1).unwrap();
    queue.push(2).unwrap();
    queue.push(3).unwrap();
    
    println!("  - 推入: 1, 2, 3");
    println!("  - 队列长度: {}", queue.len());
    
    while let Some(value) = queue.pop() {
        println!("  - 弹出: {}", value);
    }
    
    println!("  - 队列长度: {}", queue.len());
    
    // 并发操作演示
    println!("并发操作测试:");
    let queue_clone = Arc::clone(&queue);
    let mut handles = vec![];
    
    let start = Instant::now();
    
    // 5个生产者线程
    for i in 0..5 {
        let queue = Arc::clone(&queue_clone);
        let handle = thread::spawn(move || {
            for j in 0..200 {
                let _ = queue.push(i * 200 + j);
            }
        });
        handles.push(handle);
    }
    
    // 3个消费者线程
    for _ in 0..3 {
        let queue = Arc::clone(&queue_clone);
        let handle = thread::spawn(move || {
            for _ in 0..300 {
                while queue.pop().is_none() {
                    std::thread::yield_now();
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    let stats = queue.stats();
    
    println!("  - 5个生产者，3个消费者");
    println!("  - 总耗时: {:?}", duration);
    println!("  - 剩余队列长度: {}", queue.len());
    println!("  - 总入队: {}", stats.enqueued.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - 总出队: {}", stats.dequeued.load(std::sync::atomic::Ordering::Relaxed));
}

/// 演示无锁计数器
fn demo_lockfree_counter() {
    println!("\n🔢 3. 无锁计数器演示");
    
    let counter = Arc::new(LockFreeCounter::new(0));
    
    // 基本操作演示
    println!("基本操作:");
    counter.increment();
    counter.increment();
    println!("  - 递增2次，当前值: {}", counter.get());
    
    counter.decrement();
    println!("  - 递减1次，当前值: {}", counter.get());
    
    // 并发操作演示
    println!("并发操作测试:");
    let counter_clone = Arc::clone(&counter);
    let mut handles = vec![];
    
    let start = Instant::now();
    
    // 10个线程并发操作
    for i in 0..10 {
        let counter = Arc::clone(&counter_clone);
        let handle = thread::spawn(move || {
            for _ in 0..1000 {
                if i % 2 == 0 {
                    counter.increment();
                } else {
                    counter.decrement();
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    let stats = counter.stats();
    
    println!("  - 10个线程，每个操作1000次");
    println!("  - 总耗时: {:?}", duration);
    println!("  - 最终值: {}", counter.get());
    println!("  - 递增次数: {}", stats.increments.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - 递减次数: {}", stats.decrements.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - 读取次数: {}", stats.reads.load(std::sync::atomic::Ordering::Relaxed));
}

/// 并发性能测试
fn demo_concurrent_performance() {
    println!("\n⚡ 4. 并发性能测试");
    
    let map = Arc::new(LockFreeHashMap::new());
    let queue = Arc::new(LockFreeQueue::new());
    let counter = Arc::new(LockFreeCounter::new(0));
    
    let start = Instant::now();
    let mut handles = vec![];
    
    // 启动20个线程进行混合操作
    for i in 0..20 {
        let map = Arc::clone(&map);
        let queue = Arc::clone(&queue);
        let counter = Arc::clone(&counter);
        
        let handle = thread::spawn(move || {
            for j in 0..1000 {
                // HashMap操作
                let key = format!("perf_{}_{}", i, j);
                let value = format!("data_{}_{}", i, j);
                let _ = map.insert(key.clone(), value);
                let _ = map.get(&key);
                
                // 队列操作
                let _ = queue.push(i * 1000 + j);
                let _ = queue.pop();
                
                // 计数器操作
                counter.increment();
                if j % 2 == 0 {
                    counter.decrement();
                }
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    let duration = start.elapsed();
    
    println!("混合并发测试结果:");
    println!("  - 线程数: 20");
    println!("  - 每线程操作数: 1000 x 6 = 6000");
    println!("  - 总操作数: {}", 20 * 6000);
    println!("  - 总耗时: {:?}", duration);
    println!("  - QPS: {:.0} ops/sec", (20 * 6000) as f64 / duration.as_secs_f64());
    
    // 最终状态
    println!("最终状态:");
    println!("  - HashMap大小: {}", map.len());
    println!("  - 队列长度: {}", queue.len());
    println!("  - 计数器值: {}", counter.get());
    
    // 性能统计
    let map_stats = map.stats();
    let queue_stats = queue.stats();
    let counter_stats = counter.stats();
    
    println!("性能统计:");
    println!("  - HashMap CAS成功率: {:.2}%", map_stats.cas_success_rate() * 100.0);
    println!("  - 平均读取延迟: {}ns", map_stats.avg_read_latency_ns.load(std::sync::atomic::Ordering::Relaxed));
    println!("  - 队列吞吐量: {:.0} ops/sec", 
             (queue_stats.enqueued.load(std::sync::atomic::Ordering::Relaxed) + 
              queue_stats.dequeued.load(std::sync::atomic::Ordering::Relaxed)) as f64 / duration.as_secs_f64());
    println!("  - 计数器操作数: {}", 
             counter_stats.increments.load(std::sync::atomic::Ordering::Relaxed) + 
             counter_stats.decrements.load(std::sync::atomic::Ordering::Relaxed));
} 