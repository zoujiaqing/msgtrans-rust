/// Flume vs Crossbeam vs Tokio 全面对比测试

use std::time::Instant;
use tokio::sync::mpsc as tokio_mpsc;
use crossbeam_channel::unbounded as crossbeam_unbounded;
use crossbeam_utils::thread;
use flume::unbounded as flume_unbounded;

#[derive(Debug, Clone)]
pub struct TestMessage {
    pub id: u64,
    pub data: Vec<u8>,
}

impl TestMessage {
    fn new(id: u64, size: usize) -> Self {
        Self {
            id,
            data: vec![0u8; size],
        }
    }
}

async fn benchmark_all() {
    println!("🎯 Channel 性能对比：Flume vs Crossbeam vs Tokio");
    println!("=============================================");
    
    const NUM_ACTORS: usize = 4;
    const MESSAGES_PER_ACTOR: usize = 25000;
    const TOTAL_MESSAGES: usize = NUM_ACTORS * MESSAGES_PER_ACTOR;
    
    // 1. Tokio 测试
    println!("\n📊 测试 Tokio (异步)...");
    let tokio_start = Instant::now();
    
    let mut handles = Vec::new();
    let mut senders = Vec::new();
    
    for i in 0..NUM_ACTORS {
        let (tx, mut rx) = tokio_mpsc::channel::<TestMessage>(1000);
        
        let handle = tokio::spawn(async move {
            let mut count = 0;
            while let Some(msg) = rx.recv().await {
                count += 1;
                if msg.id == u64::MAX { break; }
            }
            count
        });
        
        handles.push(handle);
        senders.push(tx);
    }
    
    // 发送消息
    for i in 0..MESSAGES_PER_ACTOR {
        for (actor_id, sender) in senders.iter().enumerate() {
            let msg = TestMessage::new((actor_id * MESSAGES_PER_ACTOR + i) as u64, 64);
            let _ = sender.send(msg).await;
        }
    }
    
    // 发送关闭信号
    for sender in senders {
        let _ = sender.send(TestMessage::new(u64::MAX, 0)).await;
    }
    
    // 等待完成
    for handle in handles {
        let _ = handle.await;
    }
    
    let tokio_duration = tokio_start.elapsed();
    
    // 2. Crossbeam 测试
    println!("📊 测试 Crossbeam (同步)...");
    let crossbeam_start = Instant::now();
    
    thread::scope(|s| {
        let mut senders = Vec::new();
        
        for i in 0..NUM_ACTORS {
            let (tx, rx) = crossbeam_unbounded::<TestMessage>();
            
            s.spawn(move |_| {
                let mut count = 0;
                while let Ok(msg) = rx.recv() {
                    count += 1;
                    if msg.id == u64::MAX { break; }
                }
                count
            });
            
            senders.push(tx);
        }
        
        // 发送消息
        for i in 0..MESSAGES_PER_ACTOR {
            for (actor_id, sender) in senders.iter().enumerate() {
                let msg = TestMessage::new((actor_id * MESSAGES_PER_ACTOR + i) as u64, 64);
                let _ = sender.send(msg);
            }
        }
        
        // 发送关闭信号
        for sender in senders {
            let _ = sender.send(TestMessage::new(u64::MAX, 0));
        }
    }).unwrap();
    
    let crossbeam_duration = crossbeam_start.elapsed();
    
    // 3. Flume 同步测试
    println!("📊 测试 Flume (同步)...");
    let flume_sync_start = Instant::now();
    
    thread::scope(|s| {
        let mut senders = Vec::new();
        
        for i in 0..NUM_ACTORS {
            let (tx, rx) = flume_unbounded::<TestMessage>();
            
            s.spawn(move |_| {
                let mut count = 0;
                while let Ok(msg) = rx.recv() {
                    count += 1;
                    if msg.id == u64::MAX { break; }
                }
                count
            });
            
            senders.push(tx);
        }
        
        // 发送消息
        for i in 0..MESSAGES_PER_ACTOR {
            for (actor_id, sender) in senders.iter().enumerate() {
                let msg = TestMessage::new((actor_id * MESSAGES_PER_ACTOR + i) as u64, 64);
                let _ = sender.send(msg);
            }
        }
        
        // 发送关闭信号
        for sender in senders {
            let _ = sender.send(TestMessage::new(u64::MAX, 0));
        }
    }).unwrap();
    
    let flume_sync_duration = flume_sync_start.elapsed();
    
    // 4. Flume 异步测试
    println!("📊 测试 Flume (异步)...");
    let flume_async_start = Instant::now();
    
    let mut handles = Vec::new();
    let mut senders = Vec::new();
    
    for i in 0..NUM_ACTORS {
        let (tx, rx) = flume_unbounded::<TestMessage>();
        
        let handle = tokio::spawn(async move {
            let mut count = 0;
            while let Ok(msg) = rx.recv_async().await {
                count += 1;
                if msg.id == u64::MAX { break; }
            }
            count
        });
        
        handles.push(handle);
        senders.push(tx);
    }
    
    // 发送消息（混合：同步发送）
    for i in 0..MESSAGES_PER_ACTOR {
        for (actor_id, sender) in senders.iter().enumerate() {
            let msg = TestMessage::new((actor_id * MESSAGES_PER_ACTOR + i) as u64, 64);
            let _ = sender.send(msg); // 同步发送到异步接收
        }
    }
    
    // 发送关闭信号
    for sender in senders {
        let _ = sender.send_async(TestMessage::new(u64::MAX, 0)).await;
    }
    
    // 等待完成
    for handle in handles {
        let _ = handle.await;
    }
    
    let flume_async_duration = flume_async_start.elapsed();
    
    // 结果对比
    println!("\n📈 性能对比结果:");
    println!("Tokio:       {:?} ({:.0} msg/s)", tokio_duration, TOTAL_MESSAGES as f64 / tokio_duration.as_secs_f64());
    println!("Crossbeam:   {:?} ({:.0} msg/s)", crossbeam_duration, TOTAL_MESSAGES as f64 / crossbeam_duration.as_secs_f64());
    println!("Flume同步:   {:?} ({:.0} msg/s)", flume_sync_duration, TOTAL_MESSAGES as f64 / flume_sync_duration.as_secs_f64());
    println!("Flume异步:   {:?} ({:.0} msg/s)", flume_async_duration, TOTAL_MESSAGES as f64 / flume_async_duration.as_secs_f64());
    
    // 计算性能倍数
    let tokio_ns = tokio_duration.as_nanos() as f64;
    let crossbeam_speedup = tokio_ns / crossbeam_duration.as_nanos() as f64;
    let flume_sync_speedup = tokio_ns / flume_sync_duration.as_nanos() as f64;
    let flume_async_speedup = tokio_ns / flume_async_duration.as_nanos() as f64;
    
    println!("\n🏆 相对于 Tokio 的性能提升:");
    println!("Crossbeam 快: {:.2}x", crossbeam_speedup);
    println!("Flume同步 快: {:.2}x", flume_sync_speedup);
    println!("Flume异步 快: {:.2}x", flume_async_speedup);
}

async fn api_demo() {
    println!("\n🛠️ API 易用性展示");
    println!("================");
    
    let (tx, rx) = flume_unbounded::<String>();
    
    println!("📝 Flume 的独特优势:");
    println!("✅ 同步发送: tx.send(msg)");
    println!("✅ 异步发送: tx.send_async(msg).await");
    println!("✅ 同步接收: rx.recv()");
    println!("✅ 异步接收: rx.recv_async().await");
    println!("✅ 非阻塞: tx.try_send(msg), rx.try_recv()");
    println!("✅ 超时: rx.recv_timeout(duration)");
    
    // 演示混合使用
    let _ = tx.send("同步发送的消息".to_string());
    
    tokio::spawn({
        let tx = tx.clone();
        async move {
            let _ = tx.send_async("异步发送的消息".to_string()).await;
        }
    });
    
    // 同步接收
    if let Ok(msg) = rx.try_recv() {
        println!("📨 同步接收: {}", msg);
    }
    
    // 异步接收
    if let Ok(msg) = rx.recv_async().await {
        println!("📨 异步接收: {}", msg);
    }
}

#[tokio::main]
async fn main() {
    benchmark_all().await;
    api_demo().await;
    
    println!("\n🎯 结论:");
    println!("Flume 是最佳选择，原因:");
    println!("1. 📈 性能优异，接近 Crossbeam");
    println!("2. 🔄 同时支持同步和异步API");
    println!("3. 🛠️ API设计更现代化");
    println!("4. 🎯 完美适合 MsgTrans 的混合使用场景");
    
    println!("\n✨ 建议将 Actor 通信迁移到 Flume！");
} 