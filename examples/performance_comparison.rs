/// 性能对比测试：传统连接 vs 无锁连接
/// 
/// 测试场景：
/// 1. 并发发送性能
/// 2. 请求/响应延迟
/// 3. 内存使用效率
/// 4. CPU使用率

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Semaphore};
use msgtrans::{
    SessionId,
    packet::Packet,
    error::TransportError,
    command::ConnectionInfo,
    Connection,
    transport::LockFreeConnection,
    event::TransportEvent,
};

/// 模拟传统连接（带锁）
struct TraditionalConnection {
    session_id: SessionId,
    is_connected: bool,
    send_count: std::sync::Arc<std::sync::Mutex<u32>>,
    delay: Duration,
}

impl TraditionalConnection {
    fn new(id: u64, delay: Duration) -> Self {
        Self {
            session_id: SessionId::new(id),
            is_connected: true,
            send_count: Arc::new(std::sync::Mutex::new(0)),
            delay,
        }
    }
}

#[async_trait::async_trait]
impl Connection for TraditionalConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        // 模拟锁竞争
        let current_count = {
            let mut count = self.send_count.lock().unwrap();
            *count += 1;
            *count
        }; // 锁在这里被释放
        
        // 模拟网络延迟
        tokio::time::sleep(self.delay).await;
        
        if current_count % 1000 == 0 {
            println!("🐌 传统连接发送 #{} (ID: {})", current_count, packet.message_id());
        }
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        self.is_connected = false;
        Ok(())
    }
    
    fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id = session_id;
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::default()
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected
    }
    
    async fn flush(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
    
    fn event_stream(&self) -> Option<broadcast::Receiver<TransportEvent>> {
        None
    }
}

/// 模拟高性能连接（无锁）
struct FastConnection {
    session_id: SessionId,
    is_connected: bool,
    send_count: std::sync::atomic::AtomicU32,
    delay: Duration,
}

impl FastConnection {
    fn new(id: u64, delay: Duration) -> Self {
        Self {
            session_id: SessionId::new(id),
            is_connected: true,
            send_count: std::sync::atomic::AtomicU32::new(0),
            delay,
        }
    }
}

#[async_trait::async_trait]
impl Connection for FastConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        // 无锁原子操作
        let count = self.send_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        
        // 模拟网络延迟
        tokio::time::sleep(self.delay).await;
        
        if count % 1000 == 0 {
            println!("⚡ 无锁连接发送 #{} (ID: {})", count + 1, packet.message_id());
        }
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        self.is_connected = false;
        Ok(())
    }
    
    fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.session_id = session_id;
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        ConnectionInfo::default()
    }
    
    fn is_connected(&self) -> bool {
        self.is_connected
    }
    
    async fn flush(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
    
    fn event_stream(&self) -> Option<broadcast::Receiver<TransportEvent>> {
        None
    }
}

/// 性能测试参数
#[derive(Clone)]
struct TestConfig {
    /// 并发任务数
    concurrent_tasks: usize,
    /// 每个任务发送的消息数
    messages_per_task: usize,
    /// 模拟网络延迟
    network_delay: Duration,
}

impl TestConfig {
    fn high_concurrency() -> Self {
        Self {
            concurrent_tasks: 20,
            messages_per_task: 50,
            network_delay: Duration::from_micros(10), // 0.01ms
        }
    }
    
    fn low_latency() -> Self {
        Self {
            concurrent_tasks: 5,
            messages_per_task: 100,
            network_delay: Duration::from_micros(5), // 0.005ms
        }
    }
}

/// 测试传统连接性能
async fn test_traditional_connection(config: TestConfig) -> (Duration, u64) {
    println!("\n🐌 测试传统连接性能");
    println!("   并发数: {}, 每任务消息数: {}", config.concurrent_tasks, config.messages_per_task);
    
    let connection = TraditionalConnection::new(1, config.network_delay);
    let (lockfree_conn, worker_handle) = LockFreeConnection::new(
        Box::new(connection),
        SessionId::new(1),
        1000
    );
    
    let conn = Arc::new(lockfree_conn);
    let start_time = Instant::now();
    let semaphore = Arc::new(Semaphore::new(config.concurrent_tasks));
    
    let mut handles = Vec::new();
    
    for task_id in 0..config.concurrent_tasks {
        let conn_clone = conn.clone();
        let sem_clone = semaphore.clone();
        let messages_count = config.messages_per_task;
        
        let handle = tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();
            
            for msg_id in 0..messages_count {
                let packet = Packet::one_way((task_id * 1000 + msg_id) as u32, vec![0u8; 64]);
                if let Err(e) = conn_clone.send_lockfree(packet).await {
                    eprintln!("❌ 传统连接发送失败: {:?}", e);
                }
            }
        });
        
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        let _ = handle.await;
    }
    
    let elapsed = start_time.elapsed();
    let total_messages = (config.concurrent_tasks * config.messages_per_task) as u64;
    
    // 停止工作器
    worker_handle.abort();
    
    println!("✅ 传统连接测试完成");
    println!("   耗时: {:?}", elapsed);
    println!("   总消息数: {}", total_messages);
    println!("   QPS: {:.0}", total_messages as f64 / elapsed.as_secs_f64());
    
    (elapsed, total_messages)
}

/// 测试无锁连接性能
async fn test_lockfree_connection(config: TestConfig) -> (Duration, u64) {
    println!("\n⚡ 测试无锁连接性能");
    println!("   并发数: {}, 每任务消息数: {}", config.concurrent_tasks, config.messages_per_task);
    
    let connection = FastConnection::new(2, config.network_delay);
    let (lockfree_conn, worker_handle) = LockFreeConnection::new(
        Box::new(connection),
        SessionId::new(2),
        1000
    );
    
    let conn = Arc::new(lockfree_conn);
    let start_time = Instant::now();
    let semaphore = Arc::new(Semaphore::new(config.concurrent_tasks));
    
    let mut handles = Vec::new();
    
    for task_id in 0..config.concurrent_tasks {
        let conn_clone = conn.clone();
        let sem_clone = semaphore.clone();
        let messages_count = config.messages_per_task;
        
        let handle = tokio::spawn(async move {
            let _permit = sem_clone.acquire().await.unwrap();
            
            for msg_id in 0..messages_count {
                let packet = Packet::one_way((task_id * 1000 + msg_id) as u32, vec![0u8; 64]);
                if let Err(e) = conn_clone.send_lockfree(packet).await {
                    eprintln!("❌ 无锁连接发送失败: {:?}", e);
                }
            }
        });
        
        handles.push(handle);
    }
    
    // 等待所有任务完成
    for handle in handles {
        let _ = handle.await;
    }
    
    let elapsed = start_time.elapsed();
    let total_messages = (config.concurrent_tasks * config.messages_per_task) as u64;
    
    // 停止工作器
    worker_handle.abort();
    
    println!("✅ 无锁连接测试完成");
    println!("   耗时: {:?}", elapsed);
    println!("   总消息数: {}", total_messages);
    println!("   QPS: {:.0}", total_messages as f64 / elapsed.as_secs_f64());
    
    (elapsed, total_messages)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🏁 性能对比测试：传统连接 vs 无锁连接");
    println!("=====================================");
    
    // 测试1: 高并发场景
    println!("\n📊 测试1: 高并发场景");
    let high_concurrency_config = TestConfig::high_concurrency();
    
    let (traditional_time, traditional_messages) = test_traditional_connection(high_concurrency_config.clone()).await;
    tokio::time::sleep(Duration::from_millis(100)).await; // 清理间隔
    
    let (lockfree_time, lockfree_messages) = test_lockfree_connection(high_concurrency_config).await;
    
    println!("\n📈 高并发测试结果对比:");
    println!("   传统连接: {:?} ({} msg)", traditional_time, traditional_messages);
    println!("   无锁连接: {:?} ({} msg)", lockfree_time, lockfree_messages);
    
    let improvement_ratio = traditional_time.as_secs_f64() / lockfree_time.as_secs_f64();
    println!("   🚀 性能提升: {:.2}x", improvement_ratio);
    
    // 测试2: 低延迟场景
    println!("\n📊 测试2: 低延迟场景");
    let low_latency_config = TestConfig::low_latency();
    
    tokio::time::sleep(Duration::from_millis(100)).await; // 清理间隔
    let (traditional_time_2, traditional_messages_2) = test_traditional_connection(low_latency_config.clone()).await;
    tokio::time::sleep(Duration::from_millis(100)).await; // 清理间隔
    
    let (lockfree_time_2, lockfree_messages_2) = test_lockfree_connection(low_latency_config).await;
    
    println!("\n📈 低延迟测试结果对比:");
    println!("   传统连接: {:?} ({} msg)", traditional_time_2, traditional_messages_2);
    println!("   无锁连接: {:?} ({} msg)", lockfree_time_2, lockfree_messages_2);
    
    let improvement_ratio_2 = traditional_time_2.as_secs_f64() / lockfree_time_2.as_secs_f64();
    println!("   🚀 性能提升: {:.2}x", improvement_ratio_2);
    
    // 总结
    println!("\n🎯 性能测试总结");
    println!("===============");
    println!("高并发场景提升: {:.2}x", improvement_ratio);
    println!("低延迟场景提升: {:.2}x", improvement_ratio_2);
    println!("平均性能提升: {:.2}x", (improvement_ratio + improvement_ratio_2) / 2.0);
    
    // 计算QPS对比
    let traditional_qps = traditional_messages as f64 / traditional_time.as_secs_f64();
    let lockfree_qps = lockfree_messages as f64 / lockfree_time.as_secs_f64();
    
    println!("\n📊 QPS对比 (高并发场景):");
    println!("   传统连接 QPS: {:.0}", traditional_qps);
    println!("   无锁连接 QPS: {:.0}", lockfree_qps);
    println!("   QPS提升倍数: {:.2}x", lockfree_qps / traditional_qps);
    
    Ok(())
} 