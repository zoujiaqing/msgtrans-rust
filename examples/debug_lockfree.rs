/// 调试无锁连接卡住问题的简化程序

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

/// 最简单的测试连接
struct TestConnection {
    session_id: SessionId,
    is_connected: bool,
    send_count: std::sync::atomic::AtomicU32,
}

impl TestConnection {
    fn new(id: u64) -> Self {
        Self {
            session_id: SessionId::new(id),
            is_connected: true,
            send_count: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

#[async_trait::async_trait]
impl Connection for TestConnection {
    async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        let count = self.send_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        println!("🔄 TestConnection::send 被调用 #{} - 消息ID: {}", count + 1, packet.message_id());
        
        // 无延迟，立即返回
        println!("✅ TestConnection::send 完成 #{}", count + 1);
        Ok(())
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        println!("🔒 TestConnection::close 被调用");
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
        println!("🚿 TestConnection::flush 被调用");
        Ok(())
    }
    
    fn event_stream(&self) -> Option<broadcast::Receiver<TransportEvent>> {
        None
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🐛 调试无锁连接卡住问题");
    println!("{}", "=".repeat(50));
    
    // 第1步：创建测试连接
    println!("\n🔧 第1步：创建测试连接");
    let connection = Box::new(TestConnection::new(1));
    let session_id = connection.session_id();
    println!("✅ 测试连接创建完成，会话ID: {:?}", session_id);
    
    // 第2步：创建无锁连接
    println!("\n🔧 第2步：创建无锁连接");
    let start_time = Instant::now();
    let (lockfree_conn, worker_handle) = LockFreeConnection::new(
        connection as Box<dyn Connection>,
        session_id,
        100 // 小缓冲区
    );
    println!("✅ 无锁连接创建完成，耗时: {:?}", start_time.elapsed());
    
    // 第3步：测试单个发送
    println!("\n🔧 第3步：测试单个发送");
    let packet = Packet::one_way(1, b"test message");
    
    println!("📤 准备发送数据包...");
    let send_start = Instant::now();
    
    // 使用超时来避免无限等待
    match tokio::time::timeout(Duration::from_secs(5), lockfree_conn.send_lockfree(packet)).await {
        Ok(Ok(())) => {
            println!("✅ 发送成功，耗时: {:?}", send_start.elapsed());
        }
        Ok(Err(e)) => {
            println!("❌ 发送失败: {:?}", e);
        }
        Err(_) => {
            println!("⏰ 发送超时（5秒）");
        }
    }
    
    // 第4步：检查统计信息
    println!("\n📊 第4步：检查统计信息");
    let stats = lockfree_conn.stats();
    println!("   - 发送数据包: {}", stats.packets_sent.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 发送失败: {}", stats.send_failures.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 队列深度: {}", stats.queue_depth.load(std::sync::atomic::Ordering::Relaxed));
    println!("   - 连接状态: {}", lockfree_conn.is_connected_lockfree());
    println!("   - 队列长度: {}", lockfree_conn.queue_depth());
    
    // 第5步：关闭连接
    println!("\n🔧 第5步：关闭连接");
    match tokio::time::timeout(Duration::from_secs(3), lockfree_conn.close_lockfree()).await {
        Ok(Ok(())) => {
            println!("✅ 连接关闭成功");
        }
        Ok(Err(e)) => {
            println!("❌ 连接关闭失败: {:?}", e);
        }
        Err(_) => {
            println!("⏰ 连接关闭超时（3秒）");
        }
    }
    
    // 第6步：等待工作器结束
    println!("\n🔧 第6步：等待工作器结束");
    match tokio::time::timeout(Duration::from_secs(3), worker_handle).await {
        Ok(Ok(())) => {
            println!("✅ 工作器正常结束");
        }
        Ok(Err(e)) => {
            println!("❌ 工作器结束时出错: {:?}", e);
        }
        Err(_) => {
            println!("⏰ 工作器结束超时（3秒）");
        }
    }
    
    println!("\n🎯 调试完成！");
    Ok(())
} 