/// 现代化传输层性能基准测试
/// 
/// 测试内容：
/// 1. 新的 TransportBuilder API 性能
/// 2. 连接池和内存池的实际应用性能
/// 3. 高并发场景测试

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use msgtrans::{
    transport::{TransportBuilder, ConnectionPool, MemoryPool, BufferSize},
};
use std::time::Duration;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 基准测试：现代化API创建性能
fn bench_modern_api_creation(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("modern_api_creation");
    group.sample_size(50);
    group.measurement_time(Duration::from_secs(20));
    
    group.bench_function("transport_builder", |b| {
        b.to_async(&rt).iter(|| async {
            let transport = TransportBuilder::new()
                .build()
                .await
                .expect("Failed to create transport");
            black_box(transport)
        });
    });
    
    group.finish();
}

/// 基准测试：连接池性能
fn bench_connection_pool_performance(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("connection_pool");
    group.throughput(Throughput::Elements(1000));
    
    // 测试不同的池大小配置
    let pool_configs = [
        ("small_pool", 10, 50),
        ("medium_pool", 50, 200),
        ("large_pool", 100, 500),
    ];
    
    for (name, min_size, max_size) in pool_configs.iter() {
        group.bench_function(*name, |b| {
            b.to_async(&rt).iter(|| async {
                let mut pool = ConnectionPool::new(*min_size, *max_size);
                
                // 模拟负载波动
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
    }
    
    group.finish();
}

/// 基准测试：内存池在真实场景中的性能
fn bench_memory_pool_realistic(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_pool_realistic");
    group.throughput(Throughput::Elements(1000));
    
    // 模拟真实的消息处理场景
    group.bench_function("message_processing", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = Arc::new(MemoryPool::new());
            let mut handles = Vec::new();
            
            // 模拟10个并发消息处理器
            for worker_id in 0..10 {
                let pool_clone = pool.clone();
                let handle = tokio::spawn(async move {
                    for msg_id in 0..100 {
                        // 获取缓冲区
                        let buffer_size = match (worker_id + msg_id) % 3 {
                            0 => BufferSize::Small,
                            1 => BufferSize::Medium,
                            _ => BufferSize::Large,
                        };
                        
                        if let Ok(mut buffer) = pool_clone.get_buffer(buffer_size).await {
                            // 模拟消息处理
                            buffer.extend_from_slice(b"test message data");
                            
                            // 模拟处理时间
                            tokio::time::sleep(Duration::from_micros(10)).await;
                            
                            // 归还缓冲区
                            pool_clone.return_buffer(buffer, buffer_size).await;
                        }
                    }
                });
                handles.push(handle);
            }
            
            // 等待所有处理器完成
            for handle in handles {
                let _ = handle.await;
            }
            
            black_box(pool.status().await)
        })
    });
    
    group.finish();
}

/// 基准测试：内存池vs标准分配器
fn bench_memory_pool_vs_standard(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_allocation_comparison");
    group.throughput(Throughput::Elements(1000));
    
    // 内存池分配
    group.bench_function("memory_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = MemoryPool::new();
            let mut buffers = Vec::new();
            
            // 分配1000个缓冲区
            for i in 0..1000 {
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
    
    // 标准分配器
    group.bench_function("standard_alloc", |b| {
        b.iter(|| {
            let mut buffers = Vec::new();
            
            // 分配1000个缓冲区
            for i in 0..1000 {
                let capacity = match i % 3 {
                    0 => 1024,     // Small
                    1 => 8192,     // Medium
                    _ => 65536,    // Large
                };
                
                let buffer = bytes::BytesMut::with_capacity(capacity);
                buffers.push(buffer);
            }
            
            black_box(buffers.len())
        })
    });
    
    group.finish();
}

/// 基准测试：高并发场景
fn bench_high_concurrency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("high_concurrency");
    group.sample_size(10);  // 确保至少10个样本
    group.measurement_time(Duration::from_secs(30));
    
    let concurrency_levels = [10, 50, 100];
    
    for &concurrency in concurrency_levels.iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_transport_creation", concurrency),
            &concurrency,
            |b, &concurrency| {
                b.to_async(&rt).iter(|| async {
                    let duration = high_concurrency_test(concurrency).await;
                    black_box(duration)
                });
            },
        );
    }
    
    group.finish();
}

/// 高并发测试实现
async fn high_concurrency_test(concurrency: usize) -> Duration {
    let start = std::time::Instant::now();
    
    let (tx, mut rx) = mpsc::channel(concurrency);
    let mut handles = Vec::new();
    
    // 启动多个并发任务
    for i in 0..concurrency {
        let tx_clone = tx.clone();
        let handle = tokio::spawn(async move {
            // 模拟传输层创建和使用
            let _transport = TransportBuilder::new()
                .build()
                .await
                .expect("Failed to create transport");
            
            // 模拟一些工作
            tokio::time::sleep(Duration::from_millis(10)).await;
            
            let _ = tx_clone.send(i).await;
        });
        handles.push(handle);
    }
    
    // 等待所有任务完成
    drop(tx);
    let mut completed = 0;
    while rx.recv().await.is_some() {
        completed += 1;
    }
    
    for handle in handles {
        let _ = handle.await;
    }
    
    assert_eq!(completed, concurrency);
    start.elapsed()
}

/// 基准测试：连接池扩展算法
fn bench_pool_expansion_algorithms(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("pool_expansion");
    
    // 智能扩展
    group.bench_function("smart_expansion", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(100, 10000);
            
            // 执行多次扩展
            for _ in 0..20 {
                let _ = pool.try_expand().await;
            }
            
            black_box(pool.detailed_status().await)
        })
    });
    
    // 模拟线性扩展（对比）
    group.bench_function("linear_expansion_simulation", |b| {
        b.iter(|| {
            let mut current_size = 100;
            let increment = 100;
            
            for _ in 0..20 {
                current_size += increment;
                if current_size > 10000 {
                    current_size = 10000;
                    break;
                }
            }
            
            black_box(current_size)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_modern_api_creation,
    bench_connection_pool_performance,
    bench_memory_pool_realistic,
    bench_memory_pool_vs_standard,
    bench_high_concurrency,
    bench_pool_expansion_algorithms
);
criterion_main!(benches); 