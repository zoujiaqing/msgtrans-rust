/// 无锁连接对象 - 解决 Arc<Mutex<Connection>> 的锁竞争问题
/// 
/// 设计思路：
/// 1. 使用 Channel 队列替代直接方法调用，避免锁竞争
/// 2. 连接状态用原子变量管理
/// 3. 发送操作通过无锁队列实现
/// 4. 统计信息使用原子计数器

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, oneshot};
use crossbeam::channel::{Sender, Receiver, unbounded};
use crate::{
    SessionId, 
    packet::Packet, 
    error::TransportError, 
    command::ConnectionInfo,
    Connection,
    event::TransportEvent
};

/// 无锁连接命令
#[derive(Debug)]
pub enum LockFreeConnectionCommand {
    /// 发送数据包
    Send {
        packet: Packet,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    /// 关闭连接
    Close {
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    /// 刷新缓冲区
    Flush {
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    /// 检查连接状态
    IsConnected {
        response_tx: oneshot::Sender<bool>,
    },
    /// 获取连接信息
    GetConnectionInfo {
        response_tx: oneshot::Sender<ConnectionInfo>,
    },
}

/// 无锁连接统计
#[derive(Debug)]
pub struct LockFreeConnectionStats {
    /// 发送的数据包数量
    pub packets_sent: AtomicU64,
    /// 发送的字节数
    pub bytes_sent: AtomicU64,
    /// 发送失败次数
    pub send_failures: AtomicU64,
    /// 命令队列长度
    pub queue_depth: AtomicU64,
    /// 最后活跃时间
    pub last_activity: AtomicU64,
    /// 连接创建时间
    pub created_at: AtomicU64,
}

impl LockFreeConnectionStats {
    pub fn new() -> Self {
        let now = Instant::now().elapsed().as_nanos() as u64;
        Self {
            packets_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            send_failures: AtomicU64::new(0),
            queue_depth: AtomicU64::new(0),
            last_activity: AtomicU64::new(now),
            created_at: AtomicU64::new(now),
        }
    }
    
    pub fn record_packet_sent(&self, bytes: usize) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(bytes as u64, Ordering::Relaxed);
        self.last_activity.store(
            Instant::now().elapsed().as_nanos() as u64, 
            Ordering::Relaxed
        );
    }
    
    pub fn record_send_failure(&self) {
        self.send_failures.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn update_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
    }
}

/// 无锁连接对象 - 核心实现
/// 
/// 这个结构体完全无锁，通过消息传递实现所有操作
pub struct LockFreeConnection {
    /// 会话ID（原子操作）
    session_id: AtomicU64,
    /// 连接状态（原子操作）
    is_connected: AtomicBool,
    /// 命令发送器（无锁）
    command_tx: Sender<LockFreeConnectionCommand>,
    /// 事件广播器
    event_tx: broadcast::Sender<TransportEvent>,
    /// 统计信息（原子操作）
    stats: Arc<LockFreeConnectionStats>,
    /// 缓存的连接信息
    cached_info: ConnectionInfo,
}

impl LockFreeConnection {
    /// 创建新的无锁连接
    /// 
    /// 参数：
    /// - connection: 底层连接对象
    /// - session_id: 会话ID
    /// - buffer_size: 命令队列缓冲区大小
    pub fn new(
        mut connection: Box<dyn Connection>,
        session_id: SessionId,
        buffer_size: usize,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (command_tx, command_rx) = unbounded();
        let (event_tx, _) = broadcast::channel(buffer_size);
        let stats = Arc::new(LockFreeConnectionStats::new());
        
        let cached_info = connection.connection_info();
        
        // 🚀 桥接底层连接的事件流到无锁连接的事件通道
        let event_tx_for_bridge = event_tx.clone();
        if let Some(mut connection_events) = connection.event_stream() {
            tokio::spawn(async move {
                tracing::debug!("🌉 启动事件桥接器 (会话: {})", session_id);
                while let Ok(event) = connection_events.recv().await {
                    tracing::trace!("🌉 桥接事件: {:?}", event);
                    if let Err(_) = event_tx_for_bridge.send(event) {
                        tracing::debug!("🌉 事件桥接器停止 - 接收方已断开");
                        break;
                    }
                }
                tracing::debug!("🌉 事件桥接器结束 (会话: {})", session_id);
            });
        } else {
            tracing::debug!("📭 底层连接无事件流，跳过事件桥接");
        }
        
        let lockfree_conn = Self {
            session_id: AtomicU64::new(session_id.0),
            is_connected: AtomicBool::new(connection.is_connected()),
            command_tx,
            event_tx: event_tx.clone(),
            stats: stats.clone(),
            cached_info,
        };
        
        // 启动后台处理任务
        let handle = tokio::spawn(Self::connection_worker(
            connection,
            command_rx,
            event_tx,
            stats,
        ));
        
        (lockfree_conn, handle)
    }
    
    /// 后台连接工作器 - 处理所有实际的连接操作
    async fn connection_worker(
        mut connection: Box<dyn Connection>,
        command_rx: Receiver<LockFreeConnectionCommand>,
        event_tx: broadcast::Sender<TransportEvent>,
        stats: Arc<LockFreeConnectionStats>,
    ) {
        tracing::debug!("🚀 启动无锁连接工作器 (会话: {})", connection.session_id());
        
        // 处理命令队列
        while let Ok(command) = command_rx.recv() {
            stats.update_queue_depth(command_rx.len() as u64);
            
            match command {
                LockFreeConnectionCommand::Send { packet, response_tx } => {
                    let packet_size = packet.payload.len();
                    
                    // 为发送操作添加超时，避免工作器卡住
                    let send_result = tokio::time::timeout(
                        std::time::Duration::from_secs(5), // 5秒超时
                        connection.send(packet)
                    ).await;
                    
                    match send_result {
                        Ok(Ok(_)) => {
                            stats.record_packet_sent(packet_size);
                            let _ = response_tx.send(Ok(()));
                        }
                        Ok(Err(e)) => {
                            stats.record_send_failure();
                            let _ = response_tx.send(Err(e));
                        }
                        Err(_) => {
                            // 超时错误
                            stats.record_send_failure();
                            let _ = response_tx.send(Err(TransportError::connection_error(
                                "发送操作超时", false
                            )));
                        }
                    }
                }
                
                LockFreeConnectionCommand::Close { response_tx } => {
                    // 为关闭操作也添加超时
                    let close_result = tokio::time::timeout(
                        std::time::Duration::from_secs(3), // 3秒超时
                        connection.close()
                    ).await;
                    
                    match close_result {
                        Ok(result) => {
                            let should_break = result.is_ok();
                            let _ = response_tx.send(result);
                            
                            // 关闭后退出工作器
                            if should_break {
                                let _ = event_tx.send(TransportEvent::ConnectionClosed {
                                    reason: crate::CloseReason::Normal,
                                });
                                break;
                            }
                        }
                        Err(_) => {
                            // 超时，强制关闭
                            let _ = response_tx.send(Err(TransportError::connection_error(
                                "关闭操作超时", false
                            )));
                            break; // 超时也要退出工作器
                        }
                    }
                }
                
                LockFreeConnectionCommand::Flush { response_tx } => {
                    // 为刷新操作添加超时
                    let flush_result = tokio::time::timeout(
                        std::time::Duration::from_secs(2), // 2秒超时
                        connection.flush()
                    ).await;
                    
                    match flush_result {
                        Ok(result) => {
                            let _ = response_tx.send(result);
                        }
                        Err(_) => {
                            let _ = response_tx.send(Err(TransportError::connection_error(
                                "刷新操作超时", false
                            )));
                        }
                    }
                }
                
                LockFreeConnectionCommand::IsConnected { response_tx } => {
                    let is_connected = connection.is_connected();
                    let _ = response_tx.send(is_connected);
                }
                
                LockFreeConnectionCommand::GetConnectionInfo { response_tx } => {
                    let info = connection.connection_info();
                    let _ = response_tx.send(info);
                }
            }
        }
        
        tracing::debug!("🔚 无锁连接工作器结束 (会话: {})", connection.session_id());
    }
    
    /// 🚀 无锁发送 - 核心优化方法
    /// 
    /// 通过消息队列实现，完全避免锁竞争
    pub async fn send_lockfree(&self, packet: Packet) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = LockFreeConnectionCommand::Send {
            packet,
            response_tx,
        };
        
        // 无阻塞发送命令
        self.command_tx.send(command)
            .map_err(|_| TransportError::connection_error("连接已关闭", false))?;
        
        // 等待响应
        response_rx.await
            .map_err(|_| TransportError::connection_error("响应通道关闭", false))?
    }
    
    /// 🚀 无锁关闭
    pub async fn close_lockfree(&self) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = LockFreeConnectionCommand::Close { response_tx };
        
        self.command_tx.send(command)
            .map_err(|_| TransportError::connection_error("连接已关闭", false))?;
        
        let result = response_rx.await
            .map_err(|_| TransportError::connection_error("响应通道关闭", false))?;
        
        if result.is_ok() {
            self.is_connected.store(false, Ordering::SeqCst);
        }
        
        result
    }
    
    /// 🚀 无锁刷新
    pub async fn flush_lockfree(&self) -> Result<(), TransportError> {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = LockFreeConnectionCommand::Flush { response_tx };
        
        self.command_tx.send(command)
            .map_err(|_| TransportError::connection_error("连接已关闭", false))?;
        
        response_rx.await
            .map_err(|_| TransportError::connection_error("响应通道关闭", false))?
    }
    
    /// 🚀 无锁状态检查 - 使用原子操作，超快速
    pub fn is_connected_lockfree(&self) -> bool {
        self.is_connected.load(Ordering::Relaxed)
    }
    
    /// 获取会话ID - 原子操作
    pub fn session_id_lockfree(&self) -> SessionId {
        SessionId(self.session_id.load(Ordering::Relaxed))
    }
    
    /// 设置会话ID - 原子操作
    pub fn set_session_id_lockfree(&self, session_id: SessionId) {
        self.session_id.store(session_id.0, Ordering::SeqCst);
    }
    
    /// 获取连接信息 - 使用缓存，避免异步调用
    pub fn connection_info_lockfree(&self) -> ConnectionInfo {
        self.cached_info.clone()
    }
    
    /// 订阅事件流
    pub fn subscribe_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_tx.subscribe()
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> &LockFreeConnectionStats {
        &self.stats
    }
    
    /// 获取命令队列深度
    pub fn queue_depth(&self) -> usize {
        self.command_tx.len()
    }
    
    /// 检查连接是否健康
    pub fn is_healthy(&self) -> bool {
        self.is_connected_lockfree() && self.queue_depth() < 1000 // 队列不能太深
    }
}

/// 实现 Clone，便于在多个地方使用同一个连接
impl Clone for LockFreeConnection {
    fn clone(&self) -> Self {
        Self {
            session_id: AtomicU64::new(self.session_id.load(Ordering::Relaxed)),
            is_connected: AtomicBool::new(self.is_connected.load(Ordering::Relaxed)),
            command_tx: self.command_tx.clone(),
            event_tx: self.event_tx.clone(),
            stats: self.stats.clone(),
            cached_info: self.cached_info.clone(),
        }
    }
}

/// 为了兼容性，实现 Send + Sync
unsafe impl Send for LockFreeConnection {}
unsafe impl Sync for LockFreeConnection {}

/// 🎯 批量操作支持 - 进一步优化性能
impl LockFreeConnection {
    /// 批量发送多个数据包
    pub async fn send_batch_lockfree(&self, packets: Vec<Packet>) -> Vec<Result<(), TransportError>> {
        let mut results = Vec::with_capacity(packets.len());
        
        // 并发发送所有数据包
        let mut tasks = Vec::new();
        
        for packet in packets {
            let conn = self.clone();
            let task = tokio::spawn(async move {
                conn.send_lockfree(packet).await
            });
            tasks.push(task);
        }
        
        // 等待所有任务完成
        for task in tasks {
            match task.await {
                Ok(result) => results.push(result),
                Err(_) => results.push(Err(TransportError::connection_error(
                    "任务执行失败", 
                    false
                ))),
            }
        }
        
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::PacketType;
    use bytes::Bytes;
    
    // 模拟连接用于测试
    struct MockConnection {
        session_id: SessionId,
        is_connected: bool,
    }
    
    #[async_trait::async_trait]
    impl Connection for MockConnection {
        async fn send(&mut self, _packet: Packet) -> Result<(), TransportError> {
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
    
    #[tokio::test]
    async fn test_lockfree_connection_send() {
        let mock_conn = Box::new(MockConnection {
            session_id: SessionId(1),
            is_connected: true,
        });
        
        let (lockfree_conn, _handle) = LockFreeConnection::new(mock_conn, SessionId(1), 100);
        
        let packet = Packet {
            header: crate::packet::FixedHeader {
                version: 1,
                compression: crate::packet::CompressionType::None,
                packet_type: PacketType::Data,
                biz_type: 0,
                message_id: 1,
                ext_header_len: 0,
                payload_len: 4,
                reserved: crate::packet::ReservedFlags::empty(),
            },
            payload: Bytes::from("test"),
            ext_header: None,
        };
        
        let result = lockfree_conn.send_lockfree(packet).await;
        assert!(result.is_ok());
        
        // 检查统计信息
        assert_eq!(lockfree_conn.stats().packets_sent.load(Ordering::Relaxed), 1);
        assert_eq!(lockfree_conn.stats().bytes_sent.load(Ordering::Relaxed), 4);
    }
    
    #[tokio::test]
    async fn test_lockfree_connection_concurrent_sends() {
        let mock_conn = Box::new(MockConnection {
            session_id: SessionId(1),
            is_connected: true,
        });
        
        let (lockfree_conn, _handle) = LockFreeConnection::new(mock_conn, SessionId(1), 100);
        
        // 并发发送100个数据包
        let mut tasks = Vec::new();
        
        for i in 0..100 {
            let conn = lockfree_conn.clone();
            let task = tokio::spawn(async move {
                let packet = Packet {
                    header: crate::packet::FixedHeader {
                        version: 1,
                        compression: crate::packet::CompressionType::None,
                        packet_type: PacketType::Data,
                        biz_type: 0,
                        message_id: i,
                        ext_header_len: 0,
                        payload_len: 4,
                        reserved: crate::packet::ReservedFlags::empty(),
                    },
                    payload: Bytes::from("test"),
                    ext_header: None,
                };
                
                conn.send_lockfree(packet).await
            });
            tasks.push(task);
        }
        
        // 等待所有任务完成
        for task in tasks {
            let result = task.await.unwrap();
            assert!(result.is_ok());
        }
        
        // 检查统计信息
        assert_eq!(lockfree_conn.stats().packets_sent.load(Ordering::Relaxed), 100);
    }
} 