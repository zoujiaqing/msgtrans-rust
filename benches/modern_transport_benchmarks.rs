/// 现代化传输层性能基准测试
/// 
/// 测试内容：
/// 1. 新的 TransportBuilder API 性能
/// 2. 统一的协议接口性能对比 (TCP/WebSocket/QUIC)
/// 3. 连接池和内存池的实际应用性能
/// 4. 端到端的真实场景测试

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use msgtrans::{
    transport::TransportBuilder, 
    protocol::{TcpConfig, WebSocketConfig, QuicConfig},
    transport::{ConnectionPool, MemoryPool, BufferSize},
    event::TransportEvent,
    packet::Packet,
};
use std::time::Duration;
use bytes::BytesMut;
use std::sync::Arc;
use futures::StreamExt;
use tokio::sync::mpsc;

/// 基准测试：现代化API连接建立性能
fn bench_modern_api_connection(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("modern_api_connection");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(30));
    
    let protocols = [
        ("tcp", "127.0.0.1:18001"),
        ("websocket", "127.0.0.1:18002"), 
        ("quic", "127.0.0.1:18003"),
    ];
    
    for (protocol, addr) in protocols.iter() {
        group.bench_with_input(
            BenchmarkId::new("connection_establishment", protocol),
            &(protocol, addr),
            |b, &(protocol, addr)| {
                b.to_async(&rt).iter(|| async {
                    let duration = modern_connection_test(protocol, addr).await;
                    black_box(duration)
                });
            },
        );
    }
    
    group.finish();
}

/// 现代化连接测试实现
async fn modern_connection_test(protocol: &str, addr: &str) -> Duration {
    let start = std::time::Instant::now();
    
    // 使用新的 TransportBuilder API
    let server_transport = TransportBuilder::new()
        .build()
        .await
        .expect("Failed to create server transport");
    
    let client_transport = TransportBuilder::new()
        .build()
        .await
        .expect("Failed to create client transport");
    
    // 启动服务器
    let _server_handle = match protocol {
        "tcp" => {
            let config = TcpConfig::new(addr).expect("Failed to create TCP config")
                .with_nodelay(true)
                .with_keepalive(Some(Duration::from_secs(60)));
            server_transport.listen(config).await.expect("Failed to start TCP server")
        }
        "websocket" => {
            let config = WebSocketConfig::new(addr).expect("Failed to create WebSocket config")
                .with_path("/test".to_string());
            server_transport.listen(config).await.expect("Failed to start WebSocket server")
        }
        "quic" => {
            let config = QuicConfig::new(addr).expect("Failed to create QUIC config")
                .with_max_idle_timeout(Duration::from_secs(30));
            server_transport.listen(config).await.expect("Failed to start QUIC server")
        }
        _ => panic!("Unknown protocol: {}", protocol),
    };
    
    // 等待服务器启动
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    // 客户端连接
    let _session_id = match protocol {
        "tcp" => {
            let config = TcpConfig::new(addr).expect("Failed to create TCP config")
                .with_nodelay(true);
            client_transport.connect(config).await.expect("Failed to connect TCP")
        }
        "websocket" => {
            let config = WebSocketConfig::new(addr).expect("Failed to create WebSocket config")
                .with_path("/test".to_string());
            client_transport.connect(config).await.expect("Failed to connect WebSocket")
        }
        "quic" => {
            let config = QuicConfig::new(addr).expect("Failed to create QUIC config");
            client_transport.connect(config).await.expect("Failed to connect QUIC")
        }
        _ => panic!("Unknown protocol: {}", protocol),
    };
    
    start.elapsed()
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

/// 基准测试：高并发场景
fn bench_high_concurrency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("high_concurrency");
    group.sample_size(5);
    group.measurement_time(Duration::from_secs(60));
    
    let concurrency_levels = [10, 50, 100];
    
    for &concurrency in concurrency_levels.iter() {
        group.bench_with_input(
            BenchmarkId::new("concurrent_connections", concurrency),
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
            // 模拟连接和消息处理
            let transport = TransportBuilder::new()
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

criterion_group!(
    benches,
    bench_modern_api_connection,
    bench_connection_pool_performance,
    bench_memory_pool_realistic,
    bench_high_concurrency
);
criterion_main!(benches); 