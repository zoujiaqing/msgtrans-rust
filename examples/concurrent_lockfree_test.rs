/// 并发无锁连接测试程序
/// 
/// 专门测试并发场景下的无锁连接行为

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use msgtrans::{
    SessionId,
    packet::Packet,
    error::TransportError,
    command::ConnectionInfo,
    Connection,
    transport::LockFreeConnection,
    event::TransportEvent,
};

/// 快速测试连接（无延迟）
struct FastConnection {
    session_id: SessionId,
    is_connected: bool,
    send_count: std::sync::atomic::AtomicU32,
}

impl FastConnection {
    fn new(id: u64) -> Self {
        Self {
            session_id: SessionId::new(id),
            is_connected: true,
            send_count: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Connection for FastConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        let count = self.send_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // 添加一个很小的延迟来模拟真实网络
        tokio::time::sleep(Duration::from_micros(100)).await; // 0.1ms
        println!("✅ 快速发送完成 #{} - 消息ID: {}", count + 1, packet.message_id());
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        println!("🔒 FastConnection::close 被调用");
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 并发无锁连接测试");
    println!("{}", "=".repeat(50));
    
    // 创建无锁连接
    let connection = Box::new(FastConnection::new(1));
    let session_id = connection.session_id();
    
    let (lockfree_conn, worker_handle) = LockFreeConnection::new(
        connection as Box<dyn Connection>,
        session_id,
        1000
    );
    let conn = Arc::new(lockfree_conn);
    
    println!("✅ 无锁连接创建完成，会话ID: {:?}", session_id);
    
    // 测试1：少量并发任务
    println!("\n🧪 测试1：少量并发任务 (3个任务，每个2个包)");
    {
        let start = Instant::now();
        let mut tasks = Vec::new();
        
        for task_id in 0..3 {
            let conn_clone = conn.clone();
            let task = tokio::spawn(async move {
                println!("🔄 任务 {} 开始", task_id);
                for packet_id in 0..2 {
                    let global_id = task_id * 2 + packet_id;
                    let packet = Packet::one_way(global_id, format!("数据包-{}", global_id));
                    
                    match tokio::time::timeout(
                        Duration::from_secs(2), 
                        conn_clone.send_lockfree(packet)
                    ).await {
                        Ok(Ok(())) => println!("✅ 任务 {} 包 {} 发送成功", task_id, packet_id),
                        Ok(Err(e)) => println!("❌ 任务 {} 包 {} 发送失败: {:?}", task_id, packet_id, e),
                        Err(_) => println!("⏰ 任务 {} 包 {} 发送超时", task_id, packet_id),
                    }
                }
                println!("🏁 任务 {} 完成", task_id);
            });
            tasks.push(task);
        }
        
        // 等待所有任务完成
        println!("⏳ 等待任务完成...");
        for (i, task) in tasks.into_iter().enumerate() {
            match tokio::time::timeout(Duration::from_secs(5), task).await {
                Ok(Ok(())) => println!("✅ 任务 {} 完成", i),
                Ok(Err(e)) => println!("❌ 任务 {} 失败: {:?}", i, e),
                Err(_) => println!("⏰ 任务 {} 超时", i),
            }
        }
        
        println!("测试1完成，耗时: {:?}", start.elapsed());
    }
    
    // 检查统计信息
    println!("\n📊 当前统计:");
    let stats = conn.stats();
    println!("   - 发送数据包: {}", stats.packets_sent.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 发送失败: {}", stats.send_failures.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 队列深度: {}", conn.queue_depth());
    
    // 测试2：中等并发任务
    println!("\n🧪 测试2：中等并发任务 (5个任务，每个3个包)");
    {
        let start = Instant::now();
        let mut tasks = Vec::new();
        
        for task_id in 0..5 {
            let conn_clone = conn.clone();
            let task = tokio::spawn(async move {
                println!("🔄 任务 {} 开始", task_id);
                for packet_id in 0..3 {
                    let global_id = 100 + task_id * 3 + packet_id; // 避免ID冲突
                    let packet = Packet::one_way(global_id, format!("数据包-{}", global_id));
                    
                    match tokio::time::timeout(
                        Duration::from_secs(3), 
                        conn_clone.send_lockfree(packet)
                    ).await {
                        Ok(Ok(())) => println!("✅ 任务 {} 包 {} 发送成功", task_id, packet_id),
                        Ok(Err(e)) => println!("❌ 任务 {} 包 {} 发送失败: {:?}", task_id, packet_id, e),
                        Err(_) => println!("⏰ 任务 {} 包 {} 发送超时", task_id, packet_id),
                    }
                }
                println!("🏁 任务 {} 完成", task_id);
            });
            tasks.push(task);
        }
        
        // 等待所有任务完成
        println!("⏳ 等待任务完成...");
        for (i, task) in tasks.into_iter().enumerate() {
            match tokio::time::timeout(Duration::from_secs(10), task).await {
                Ok(Ok(())) => println!("✅ 任务 {} 完成", i),
                Ok(Err(e)) => println!("❌ 任务 {} 失败: {:?}", i, e),
                Err(_) => println!("⏰ 任务 {} 超时", i),
            }
        }
        
        println!("测试2完成，耗时: {:?}", start.elapsed());
    }
    
    // 最终统计
    println!("\n📊 最终统计:");
    let stats = conn.stats();
    println!("   - 发送数据包: {}", stats.packets_sent.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 发送失败: {}", stats.send_failures.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 队列深度: {}", conn.queue_depth());
    println!("   - 连接状态: {}", conn.is_connected_lockfree());
    
    // 清理
    println!("\n🧹 清理资源...");
    match tokio::time::timeout(Duration::from_secs(3), conn.close_lockfree()).await {
        Ok(Ok(())) => println!("✅ 连接关闭成功"),
        Ok(Err(e)) => println!("❌ 连接关闭失败: {:?}", e),
        Err(_) => println!("⏰ 连接关闭超时"),
    }
    
    match tokio::time::timeout(Duration::from_secs(3), worker_handle).await {
        Ok(Ok(())) => println!("✅ 工作器正常结束"),
        Ok(Err(e)) => println!("❌ 工作器结束出错: {:?}", e),
        Err(_) => println!("⏰ 工作器结束超时"),
    }
    
    println!("\n🎯 并发测试完成！");
    Ok(())
} 