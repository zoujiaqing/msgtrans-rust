/// Phase 2 性能基准测试
/// 
/// 对比测试：
/// 1. 智能扩展 vs 固定大小池
/// 2. 零拷贝内存池 vs 标准分配器
/// 3. 不同负载模式下的性能表现

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use msgtrans::{ConnectionPool, MemoryPool, transport::BufferSize};
use std::time::Duration;
use bytes::BytesMut;
use std::sync::Arc;

/// 基准测试：智能扩展 vs 固定池
fn bench_smart_vs_fixed_pool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("pool_comparison");
    group.throughput(Throughput::Elements(1000));
    
    // 智能扩展池
    group.bench_function("smart_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(100, 2000);
            
            // 模拟负载波动
            for _ in 0..10 {
                let _ = pool.try_expand().await;
            }
            for _ in 0..5 {
                let _ = pool.try_shrink().await;
            }
            
            black_box(pool.detailed_status().await)
        })
    });
    
    // 固定大小池（作为对比）
    group.bench_function("fixed_pool", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = ConnectionPool::new(1000, 1000); // 固定大小
            black_box(pool.detailed_status().await)
        })
    });
    
    group.finish();
}

/// 基准测试：零拷贝内存池 vs 标准分配
fn bench_memory_pool_vs_standard(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_allocation");
    group.throughput(Throughput::Elements(1000));
    
    // 零拷贝内存池
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
                
                let buffer = pool.get_buffer(size);
                buffers.push((buffer, size));
            }
            
            // 归还缓冲区
            for (buffer, size) in buffers {
                pool.return_buffer(buffer, size);
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
                
                let buffer = BytesMut::with_capacity(capacity);
                buffers.push(buffer);
            }
            
            black_box(buffers.len())
        })
    });
    
    group.finish();
}

/// 基准测试：并发场景下的性能
fn bench_concurrent_scenarios(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("concurrent_performance");
    
    // 低并发场景 (10个并发)
    group.bench_function("low_concurrency", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = Arc::new(MemoryPool::new());
            let mut handles = Vec::new();
            
            for _ in 0..10 {
                let pool_clone = pool.clone();
                let handle = tokio::spawn(async move {
                    for _ in 0..100 {
                        let buffer = pool_clone.get_buffer(BufferSize::Medium);
                        pool_clone.return_buffer(buffer, BufferSize::Medium);
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                let _ = handle.await;
            }
            
            black_box(pool.status().await)
        })
    });
    
    // 高并发场景 (100个并发)
    group.bench_function("high_concurrency", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = Arc::new(MemoryPool::new());
            let mut handles = Vec::new();
            
            for _ in 0..100 {
                let pool_clone = pool.clone();
                let handle = tokio::spawn(async move {
                    for _ in 0..10 {
                        let buffer = pool_clone.get_buffer(BufferSize::Small);
                        pool_clone.return_buffer(buffer, BufferSize::Small);
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                let _ = handle.await;
            }
            
            black_box(pool.status().await)
        })
    });
    
    group.finish();
}

/// 基准测试：扩展算法效率
fn bench_expansion_algorithms(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("expansion_efficiency");
    
    // 渐进式扩展
    group.bench_function("progressive_expansion", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(100, 10000);
            
            // 执行多次扩展
            for _ in 0..10 {
                let _ = pool.try_expand().await;
            }
            
            black_box(pool.detailed_status().await)
        })
    });
    
    // 线性扩展（对比）
    group.bench_function("linear_expansion", |b| {
        b.to_async(&rt).iter(|| async {
            // 模拟线性扩展（每次增加固定数量）
            let mut current_size = 100;
            let increment = 100;
            
            for _ in 0..10 {
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

/// 基准测试：内存使用效率
fn bench_memory_efficiency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("memory_efficiency");
    
    // 池化内存使用
    group.bench_function("pooled_memory", |b| {
        b.to_async(&rt).iter(|| async {
            let pool = MemoryPool::new();
            
            // 分配并归还大量缓冲区
            for _ in 0..1000 {
                let buffer = pool.get_buffer(BufferSize::Medium);
                pool.return_buffer(buffer, BufferSize::Medium);
            }
            
            black_box(pool.status().await)
        })
    });
    
    // 直接分配内存
    group.bench_function("direct_allocation", |b| {
        b.iter(|| {
            // 直接分配1000个缓冲区
            for _ in 0..1000 {
                let _buffer = BytesMut::with_capacity(8192);
                // 缓冲区在此处被丢弃
            }
            
            black_box(1000)
        })
    });
    
    group.finish();
}

/// 基准测试：实际负载模拟
fn bench_realistic_workload(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("realistic_workload");
    group.sample_size(50); // 减少样本数量，因为这是一个较重的测试
    
    // Web服务器负载模拟
    group.bench_function("web_server_load", |b| {
        b.to_async(&rt).iter(|| async {
            let mut pool = ConnectionPool::new(50, 1000);
            let memory_pool = pool.memory_pool();
            
            // 模拟Web服务器负载：突发流量 + 持续请求
            let mut handles = Vec::new();
            
            // 突发流量（100个并发请求）
            for _ in 0..100 {
                let pool_clone = memory_pool.clone();
                let handle = tokio::spawn(async move {
                    // 模拟HTTP请求处理
                    let request_buffer = pool_clone.get_buffer(BufferSize::Small);
                    let response_buffer = pool_clone.get_buffer(BufferSize::Medium);
                    
                    // 模拟处理时间
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    
                    pool_clone.return_buffer(request_buffer, BufferSize::Small);
                    pool_clone.return_buffer(response_buffer, BufferSize::Medium);
                });
                handles.push(handle);
            }
            
            // 等待所有请求完成
            for handle in handles {
                let _ = handle.await;
            }
            
            // 触发池调整
            let _ = pool.try_expand().await;
            let _ = pool.try_shrink().await;
            
            black_box(pool.detailed_status().await)
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_smart_vs_fixed_pool,
    bench_memory_pool_vs_standard,
    bench_concurrent_scenarios,
    bench_expansion_algorithms,
    bench_memory_efficiency,
    bench_realistic_workload
);

criterion_main!(benches); 