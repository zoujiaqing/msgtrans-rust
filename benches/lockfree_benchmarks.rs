/// 第一阶段无锁优化基准测试
/// 
/// 对比测试：
/// 1. 无锁HashMap vs Arc<RwLock<HashMap>>
/// 2. 无锁Queue vs RwLock<VecDeque>
/// 3. 无锁Counter vs RwLock<usize>
/// 4. 连接池无锁优化效果

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use msgtrans::transport::{
    ConnectionPool, MemoryPool, BufferSize,
    lockfree::{LockFreeHashMap, LockFreeQueue, LockFreeCounter},
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use bytes::BytesMut;

/// 基准测试：无锁HashMap vs RwLock<HashMap>
fn bench_hashmap_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("hashmap_comparison");
    group.throughput(Throughput::Elements(10000));
    
    // 无锁HashMap
    group.bench_function("lockfree_hashmap", |b| {
        b.iter(|| {
            let map = Arc::new(LockFreeHashMap::new());
            let mut handles = vec![];
            
            // 并发写入
            for i in 0..10 {
                let map_clone = Arc::clone(&map);
                let handle = thread::spawn(move || {
                    for j in 0..1000 {
                        let key = format!("key_{}", i * 1000 + j);
                        let value = format!("value_{}", i * 1000 + j);
                        let _ = map_clone.insert(key, value);
                    }
                });
                handles.push(handle);
            }
            
            // 并发读取
            for i in 0..5 {
                let map_clone = Arc::clone(&map);
                let handle = thread::spawn(move || {
                    for j in 0..2000 {
                        let key = format!("key_{}", j);
                        let _ = map_clone.get(&key);
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(map.len())
        })
    });
    
    // 标准RwLock<HashMap>
    group.bench_function("rwlock_hashmap", |b| {
        b.iter(|| {
            let map = Arc::new(RwLock::new(HashMap::<String, String>::new()));
            let mut handles = vec![];
            
            // 并发写入
            for i in 0..10 {
                let map_clone = Arc::clone(&map);
                let handle = thread::spawn(move || {
                    for j in 0..1000 {
                        let key = format!("key_{}", i * 1000 + j);
                        let value = format!("value_{}", i * 1000 + j);
                        let mut map = map_clone.write().unwrap();
                        map.insert(key, value);
                    }
                });
                handles.push(handle);
            }
            
            // 并发读取
            for i in 0..5 {
                let map_clone = Arc::clone(&map);
                let handle = thread::spawn(move || {
                    for j in 0..2000 {
                        let key = format!("key_{}", j);
                        let map = map_clone.read().unwrap();
                        let _ = map.get(&key);
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            let map = map.read().unwrap();
            black_box(map.len())
        })
    });
    
    group.finish();
}

/// 基准测试：无锁队列 vs RwLock<VecDeque>
fn bench_queue_comparison(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("queue_comparison");
    group.throughput(Throughput::Elements(100000));
    
    // 无锁队列
    group.bench_function("lockfree_queue", |b| {
        b.iter(|| {
            let queue = Arc::new(LockFreeQueue::new());
            let mut handles = vec![];
            
            // 并发写入
            for i in 0..10 {
                let queue_clone = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for j in 0..10000 {
                        let _ = queue_clone.push(i * 10000 + j);
                    }
                });
                handles.push(handle);
            }
            
            // 并发读取
            for _ in 0..5 {
                let queue_clone = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for _ in 0..20000 {
                        let _ = queue_clone.pop();
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(queue.len())
        })
    });
    
    // 标准RwLock<VecDeque>
    group.bench_function("rwlock_vecdeque", |b| {
        b.iter(|| {
            let queue = Arc::new(RwLock::new(std::collections::VecDeque::<i32>::new()));
            let mut handles = vec![];
            
            // 并发写入
            for i in 0..10 {
                let queue_clone = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for j in 0..10000 {
                        let mut queue = queue_clone.write().unwrap();
                        queue.push_back(i * 10000 + j);
                    }
                });
                handles.push(handle);
            }
            
            // 并发读取
            for _ in 0..5 {
                let queue_clone = Arc::clone(&queue);
                let handle = thread::spawn(move || {
                    for _ in 0..20000 {
                        let mut queue = queue_clone.write().unwrap();
                        let _ = queue.pop_front();
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            let queue = queue.read().unwrap();
            black_box(queue.len())
        })
    });
    
    group.finish();
}

/// 基准测试：连接池无锁优化
fn bench_connection_pool_lockfree(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("connection_pool_lockfree");
    group.throughput(Throughput::Elements(1000));
    
    // 无锁连接池
    group.bench_function("lockfree_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(100, 5000)
                .with_lockfree_optimization();
            
            // 模拟多次扩展
            for _ in 0..10 {
                let _ = pool.try_expand().await;
            }
            
            // 模拟收缩
            for _ in 0..5 {
                let _ = pool.try_shrink().await;
            }
            
            black_box(pool.detailed_status().await)
        })
    });
    
    // 标准连接池
    group.bench_function("standard_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(100, 5000);
            
            // 模拟多次扩展
            for _ in 0..10 {
                let _ = pool.try_expand().await;
            }
            
            // 模拟收缩
            for _ in 0..5 {
                let _ = pool.try_shrink().await;
            }
            
            black_box(pool.detailed_status().await)
        })
    });
    
    group.finish();
}

/// 基准测试：内存池无锁优化
fn bench_memory_pool_lockfree(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("memory_pool_lockfree");
    group.throughput(Throughput::Elements(10000));
    
    // 暂时注释掉还未完全实现的无锁内存池测试
    /*
    // 无锁内存池
    group.bench_function("lockfree_memory_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = MemoryPool::new().with_lockfree_optimization();
            let mut buffers = Vec::new();
            
            // 分配10000个缓冲区
            for i in 0..10000 {
                let size = match i % 3 {
                    0 => BufferSize::Small,
                    1 => BufferSize::Medium,
                    _ => BufferSize::Large,
                };
                
                if let Ok(buffer) = pool.get_buffer(size).await {
                    buffers.push((buffer, size));
                }
            }
            
            // 归还缓冲区
            for (buffer, size) in buffers {
                pool.return_buffer(buffer, size).await;
            }
            
            black_box(pool.status().await)
        })
    });
    */
    
    // 标准内存池
    group.bench_function("standard_memory_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = MemoryPool::new();
            let mut buffers = Vec::new();
            
            // 分配10000个缓冲区
            for i in 0..10000 {
                let size = match i % 3 {
                    0 => BufferSize::Small,
                    1 => BufferSize::Medium,
                    _ => BufferSize::Large,
                };
                
                if let Ok(buffer) = pool.get_buffer(size).await {
                    buffers.push((buffer, size));
                }
            }
            
            // 归还缓冲区
            for (buffer, size) in buffers {
                pool.return_buffer(buffer, size).await;
            }
            
            black_box(pool.status().await)
        })
    });
    
    group.finish();
}

/// 基准测试：读写延迟对比
fn bench_latency_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_comparison");
    
    // 无锁HashMap读取延迟
    group.bench_function("lockfree_read_latency", |b| {
        let map = Arc::new(LockFreeHashMap::new());
        
        // 预填充数据
        for i in 0..10000 {
            let _ = map.insert(format!("key_{}", i), format!("value_{}", i));
        }
        
        b.iter(|| {
            for i in 0..1000 {
                let key = format!("key_{}", i);
                black_box(map.get(&key));
            }
        })
    });
    
    // RwLock<HashMap>读取延迟
    group.bench_function("rwlock_read_latency", |b| {
        let map = Arc::new(RwLock::new(HashMap::<String, String>::new()));
        
        // 预填充数据
        {
            let mut map = map.write().unwrap();
            for i in 0..10000 {
                map.insert(format!("key_{}", i), format!("value_{}", i));
            }
        }
        
        b.iter(|| {
            for i in 0..1000 {
                let key = format!("key_{}", i);
                let map = map.read().unwrap();
                black_box(map.get(&key));
            }
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_hashmap_comparison,
    bench_queue_comparison,
    bench_connection_pool_lockfree,
    bench_memory_pool_lockfree,
    bench_latency_comparison
);
criterion_main!(benches); 