/// Channel性能对比基准测试
/// 
/// 对比：
/// 1. crossbeam-channel vs tokio::sync::mpsc
/// 2. 同步vs异步场景
/// 3. 单生产者vs多生产者
/// 4. 内存使用和延迟

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use crossbeam_channel::{unbounded as crossbeam_unbounded, bounded as crossbeam_bounded};
use tokio::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// 测试数据
#[derive(Clone, Debug)]
struct TestMessage {
    id: u64,
    data: Vec<u8>,
    timestamp: u64,
}

impl TestMessage {
    fn new(id: u64, size: usize) -> Self {
        Self {
            id,
            data: vec![0u8; size],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos() as u64,
        }
    }
}

/// 基准测试：无界队列吞吐量对比
fn bench_unbounded_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("unbounded_throughput");
    group.throughput(Throughput::Elements(100000));
    
    // crossbeam-channel 无界队列
    group.bench_function("crossbeam_unbounded", |b| {
        b.iter(|| {
            let (tx, rx) = crossbeam_unbounded();
            let mut handles = vec![];
            
            // 5个生产者
            for i in 0..5 {
                let tx = tx.clone();
                let handle = thread::spawn(move || {
                    for j in 0..20000 {
                        let msg = TestMessage::new((i * 20000 + j) as u64, 64);
                        let _ = tx.send(msg);
                    }
                });
                handles.push(handle);
            }
            
            // 3个消费者
            for _ in 0..3 {
                let rx = rx.clone();
                let handle = thread::spawn(move || {
                    let mut count = 0;
                    while count < 33333 {
                        if let Ok(_msg) = rx.try_recv() {
                            count += 1;
                        } else {
                            thread::yield_now();
                        }
                    }
                });
                handles.push(handle);
            }
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(())
        })
    });
    
    // tokio::sync::mpsc 无界队列 (同步测试)
    group.bench_function("tokio_mpsc_sync", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let mut handles = vec![];
                
                // 5个生产者
                for i in 0..5 {
                    let tx = tx.clone();
                    let handle = tokio::spawn(async move {
                        for j in 0..20000 {
                            let msg = TestMessage::new((i * 20000 + j) as u64, 64);
                            let _ = tx.send(msg);
                        }
                    });
                    handles.push(handle);
                }
                
                // 1个消费者 (mpsc只能单消费者)
                let consumer_handle = tokio::spawn(async move {
                    let mut count = 0;
                    while count < 100000 {
                        if let Some(_msg) = rx.recv().await {
                            count += 1;
                        }
                    }
                });
                
                for handle in handles {
                    handle.await.unwrap();
                }
                consumer_handle.await.unwrap();
                
                black_box(())
            })
        })
    });
    
    group.finish();
}

/// 基准测试：有界队列性能对比
fn bench_bounded_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("bounded_performance");
    group.throughput(Throughput::Elements(50000));
    
    // crossbeam-channel 有界队列
    group.bench_function("crossbeam_bounded_1000", |b| {
        b.iter(|| {
            let (tx, rx) = crossbeam_bounded(1000);
            let mut handles = vec![];
            
            // 生产者
            let tx_clone = tx.clone();
            let producer = thread::spawn(move || {
                for i in 0..50000 {
                    let msg = TestMessage::new(i, 128);
                    let _ = tx_clone.send(msg); // 可能阻塞
                }
            });
            handles.push(producer);
            
            // 消费者
            let consumer = thread::spawn(move || {
                for _ in 0..50000 {
                    let _ = rx.recv().unwrap();
                }
            });
            handles.push(consumer);
            
            for handle in handles {
                handle.join().unwrap();
            }
            
            black_box(())
        })
    });
    
    // tokio::sync::mpsc 有界队列
    group.bench_function("tokio_mpsc_bounded_1000", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::channel(1000);
                
                let producer = tokio::spawn(async move {
                    for i in 0..50000 {
                        let msg = TestMessage::new(i, 128);
                        let _ = tx.send(msg).await; // 异步等待
                    }
                });
                
                let consumer = tokio::spawn(async move {
                    for _ in 0..50000 {
                        let _ = rx.recv().await.unwrap();
                    }
                });
                
                tokio::join!(producer, consumer);
                
                black_box(())
            })
        })
    });
    
    group.finish();
}

/// 基准测试：延迟对比
fn bench_latency_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("latency_comparison");
    
    // crossbeam-channel 延迟测试
    group.bench_function("crossbeam_latency", |b| {
        b.iter(|| {
            let (tx, rx) = crossbeam_unbounded();
            let start = Instant::now();
            
            let sender = thread::spawn(move || {
                for i in 0..1000 {
                    let msg = TestMessage::new(i, 32);
                    let _ = tx.send(msg);
                }
            });
            
            let receiver = thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = rx.recv().unwrap();
                }
            });
            
            sender.join().unwrap();
            receiver.join().unwrap();
            
            black_box(start.elapsed())
        })
    });
    
    // tokio::sync::mpsc 延迟测试
    group.bench_function("tokio_mpsc_latency", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::unbounded_channel();
                let start = Instant::now();
                
                let sender = tokio::spawn(async move {
                    for i in 0..1000 {
                        let msg = TestMessage::new(i, 32);
                        let _ = tx.send(msg);
                    }
                });
                
                let receiver = tokio::spawn(async move {
                    for _ in 0..1000 {
                        let _ = rx.recv().await.unwrap();
                    }
                });
                
                tokio::join!(sender, receiver);
                
                black_box(start.elapsed())
            })
        })
    });
    
    group.finish();
}

/// 基准测试：内存使用对比
fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");
    
    // crossbeam-channel 内存测试
    group.bench_function("crossbeam_memory", |b| {
        b.iter(|| {
            let (tx, rx) = crossbeam_unbounded();
            
            // 发送大量消息但不接收，测试内存积累
            for i in 0..10000 {
                let msg = TestMessage::new(i, 256);
                let _ = tx.send(msg);
            }
            
            // 逐步接收，测试内存释放
            for _ in 0..10000 {
                let _ = rx.try_recv();
            }
            
            black_box(())
        })
    });
    
    // tokio::sync::mpsc 内存测试
    group.bench_function("tokio_mpsc_memory", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::unbounded_channel();
                
                // 发送大量消息但不接收
                for i in 0..10000 {
                    let msg = TestMessage::new(i, 256);
                    let _ = tx.send(msg);
                }
                
                // 逐步接收
                for _ in 0..10000 {
                    let _ = rx.try_recv();
                }
                
                black_box(())
            })
        })
    });
    
    group.finish();
}

/// 基准测试：异步集成性能
fn bench_async_integration(c: &mut Criterion) {
    let mut group = c.benchmark_group("async_integration");
    group.throughput(Throughput::Elements(10000));
    
    // crossbeam-channel 在异步环境中的使用
    group.bench_function("crossbeam_in_async", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, rx) = crossbeam_unbounded();
                
                // 使用 tokio::task::spawn_blocking 来处理同步channel
                let producer = tokio::task::spawn_blocking({
                    let tx = tx.clone();
                    move || {
                        for i in 0..10000 {
                            let msg = TestMessage::new(i, 64);
                            let _ = tx.send(msg);
                        }
                    }
                });
                
                let consumer = tokio::task::spawn_blocking(move || {
                    for _ in 0..10000 {
                        let _ = rx.recv().unwrap();
                    }
                });
                
                tokio::join!(producer, consumer);
                
                black_box(())
            })
        })
    });
    
    // 原生 tokio::sync::mpsc
    group.bench_function("tokio_mpsc_native", |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let (tx, mut rx) = mpsc::unbounded_channel();
                
                let producer = tokio::spawn(async move {
                    for i in 0..10000 {
                        let msg = TestMessage::new(i, 64);
                        let _ = tx.send(msg);
                    }
                });
                
                let consumer = tokio::spawn(async move {
                    for _ in 0..10000 {
                        let _ = rx.recv().await.unwrap();
                    }
                });
                
                tokio::join!(producer, consumer);
                
                black_box(())
            })
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    bench_unbounded_throughput,
    bench_bounded_performance,
    bench_latency_comparison,
    bench_memory_usage,
    bench_async_integration
);
criterion_main!(benches); 