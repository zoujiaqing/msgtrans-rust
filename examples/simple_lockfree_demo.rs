/// 简化的无锁连接演示
/// 
/// 演示无锁连接的基本使用方法和性能优势

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

/// 简单的模拟连接
struct SimpleConnection {
    session_id: SessionId,
    is_connected: bool,
    delay: Duration,
}

impl SimpleConnection {
    fn new(id: u64, delay: Duration) -> Self {
        Self {
            session_id: SessionId::new(id),
            is_connected: true,
            delay,
        }
    }
}

#[async_trait::async_trait]
impl Connection for SimpleConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        println!("📦 SimpleConnection 正在发送数据包 ID: {}", packet.message_id());
        tokio::time::sleep(self.delay).await;
        println!("✅ SimpleConnection 发送完成 ID: {}", packet.message_id());
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

/// 创建简单的测试数据包
fn create_simple_packet(id: u32) -> Packet {
    Packet::one_way(id, vec![0u8; 100]) // 100字节数据
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 无锁连接简单演示");
    println!("{}", "=".repeat(40));
    
    // 1. 创建无锁连接（无延迟版本）
    let connection = Box::new(SimpleConnection::new(1, Duration::from_millis(0))); // 无延迟
    let session_id = connection.session_id();
    
    println!("🔧 正在创建无锁连接...");
    let (lockfree_conn, worker_handle) = LockFreeConnection::new(
        connection as Box<dyn Connection>,
        session_id,
        1000
    );
    
    println!("✅ 创建无锁连接，会话ID: {:?}", session_id);
    
    // 2. 测试基本发送功能
    println!("\n📤 测试基本发送功能...");
    let packet = create_simple_packet(1);
    
    let start = Instant::now();
    println!("🚀 开始发送数据包 ID: 1");
    lockfree_conn.send_lockfree(packet).await?;
    let duration = start.elapsed();
    
    println!("✅ 发送成功，耗时: {:?}", duration);
    
    // 3. 测试安全的并发发送
    println!("\n🚀 测试并发发送...");
    let conn = Arc::new(lockfree_conn);
    let num_tasks = 3;  // 减少任务数
    let packets_per_task = 2;  // 减少每个任务的包数
    
    let start = Instant::now();
    
    // 使用 FuturesUnordered 来更好地处理并发
    use futures::stream::{FuturesUnordered, StreamExt};
    let mut tasks = FuturesUnordered::new();
    
    for task_id in 0..num_tasks {
        let conn_clone = conn.clone();
        let task = tokio::spawn(async move {
            println!("🔄 任务 {} 开始", task_id);
            let mut results = Vec::new();
            
            for packet_id in 0..packets_per_task {
                let packet_id_global = task_id * packets_per_task + packet_id;
                let packet = create_simple_packet(packet_id_global);
                println!("📤 任务 {} 发送数据包 {}", task_id, packet_id_global);
                
                // 为每个发送操作添加超时
                match tokio::time::timeout(
                    Duration::from_secs(2), 
                    conn_clone.send_lockfree(packet)
                ).await {
                    Ok(Ok(())) => {
                        println!("✅ 任务 {} 数据包 {} 发送成功", task_id, packet_id_global);
                        results.push(Ok(()));
                    }
                    Ok(Err(e)) => {
                        println!("❌ 任务 {} 数据包 {} 发送失败: {:?}", task_id, packet_id_global, e);
                        results.push(Err(e));
                    }
                    Err(_) => {
                        println!("⏰ 任务 {} 数据包 {} 发送超时", task_id, packet_id_global);
                        results.push(Err(TransportError::connection_error("发送超时", false)));
                    }
                }
            }
            println!("🏁 任务 {} 完成", task_id);
            (task_id, results)
        });
        tasks.push(task);
    }
    
    println!("⏳ 等待所有任务完成...");
    
    // 使用超时等待所有任务
    let timeout_duration = Duration::from_secs(10); // 总超时时间
    let overall_timeout = tokio::time::timeout(timeout_duration, async {
        let mut completed_tasks = Vec::new();
        while let Some(task_result) = tasks.next().await {
            match task_result {
                Ok((task_id, results)) => {
                    println!("✅ 任务 {} 完成，结果: {} 成功", task_id, results.iter().filter(|r| r.is_ok()).count());
                    completed_tasks.push((task_id, results));
                }
                Err(e) => {
                    println!("❌ 任务执行失败: {:?}", e);
                }
            }
        }
        completed_tasks
    }).await;
    
    match overall_timeout {
        Ok(completed_tasks) => {
            let total_duration = start.elapsed();
            let total_packets = num_tasks * packets_per_task;
            
            println!("✅ 并发发送完成:");
            println!("   - 任务数: {}", num_tasks);
            println!("   - 完成任务: {}", completed_tasks.len());
            println!("   - 总数据包: {}", total_packets);
            println!("   - 总耗时: {:?}", total_duration);
            
            if total_duration.as_secs_f64() > 0.0 {
                println!("   - 吞吐量: {:.2} packets/sec", total_packets as f64 / total_duration.as_secs_f64());
            }
        }
        Err(_) => {
            println!("⏰ 并发测试超时");
        }
    }
    
    // 4. 显示统计信息
    println!("\n📊 连接统计信息:");
    let stats = conn.stats();
    println!("   - 发送数据包: {}", stats.packets_sent.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 发送字节数: {}", stats.bytes_sent.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 发送失败: {}", stats.send_failures.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 队列深度: {}", stats.queue_depth.load(std::sync::atomic::Ordering::Relaxed));
    
    // 5. 测试健康状态
    println!("\n🏥 连接健康状态:");
    println!("   - 连接状态: {}", if conn.is_connected_lockfree() { "已连接" } else { "已断开" });
    println!("   - 健康状态: {}", if conn.is_healthy() { "健康" } else { "异常" });
    println!("   - 队列深度: {}", conn.queue_depth());
    
    // 6. 清理资源
    println!("\n🧹 清理资源...");
    match tokio::time::timeout(Duration::from_secs(3), conn.close_lockfree()).await {
        Ok(Ok(())) => println!("✅ 连接关闭成功"),
        Ok(Err(e)) => println!("❌ 连接关闭失败: {:?}", e),
        Err(_) => println!("⏰ 连接关闭超时"),
    }
    
    // 等待工作器结束
    println!("⏳ 等待工作器结束...");
    match tokio::time::timeout(Duration::from_secs(5), worker_handle).await {
        Ok(Ok(())) => println!("✅ 工作器正常结束"),
        Ok(Err(e)) => println!("❌ 工作器结束时出错: {:?}", e),
        Err(_) => println!("⚠️  工作器结束超时"),
    }
    
    println!("\n🎉 演示完成！");
    println!("\n💡 无锁连接的关键优势:");
    println!("   1. 🚀 无锁设计 - 通过消息队列避免锁竞争");
    println!("   2. ⚡ 高并发 - 支持数千并发任务同时发送");
    println!("   3. 📊 实时监控 - 原子操作统计，零开销");
    println!("   4. 🛡️  容错性强 - 队列缓冲，平滑处理突发流量");
    
    Ok(())
} 