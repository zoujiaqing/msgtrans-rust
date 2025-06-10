/// Phase 3.2.2: 双管道Actor处理优化
/// 
/// 核心优化：
/// 1. 数据管道与命令管道分离
/// 2. 批量数据处理
/// 3. 专门化处理器
/// 4. Flume高性能通信

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use flume::{Sender as FlumeSender, Receiver as FlumeReceiver};
use tokio::task::JoinHandle;
use crate::{
    error::TransportError,
    packet::Packet,
    SessionId,
    command::{ConnectionInfo, ProtocolType, ConnectionState},
};
use super::protocol_adapter_v2::{FlumePoweredProtocolAdapter, ProtocolEvent};

/// Actor命令类型
#[derive(Debug, Clone)]
pub enum ActorCommand {
    ProcessData(Packet),          // 处理数据包
    SendPacket(Packet),           // 发送数据包
    UpdateSession(SessionId),     // 更新会话
    GetStats,                     // 获取统计
    Shutdown,                     // 关闭Actor
    HealthCheck,                  // 健康检查
}

/// Actor事件类型
#[derive(Debug, Clone)]  
pub enum ActorEvent {
    DataProcessed(usize),         // 数据已处理，参数为包数量
    BatchCompleted(usize),        // 批次完成，参数为批次大小
    StatsReport(ActorStats),      // 统计报告
    Error(String),                // 错误事件
    Shutdown,                     // 关闭事件
}

/// Actor统计
#[derive(Debug, Clone)]
pub struct ActorStats {
    pub data_packets_processed: u64,
    pub commands_processed: u64,
    pub total_batches: u64,
    pub average_batch_size: f64,
    pub processing_latency_ns: u64,
    pub uptime_seconds: u64,
    pub error_count: u64,
}

/// LockFree Actor统计
#[derive(Debug)]
pub struct LockFreeActorStats {
    // 数据处理统计
    pub data_packets_processed: AtomicU64,
    pub data_bytes_processed: AtomicU64,
    pub batch_operations: AtomicU64,
    pub total_batch_size: AtomicU64,
    pub max_batch_size: AtomicUsize,
    
    // 命令处理统计
    pub commands_processed: AtomicU64,
    pub command_latency_ns: AtomicU64,
    
    // 性能统计
    pub total_processing_time_ns: AtomicU64,
    pub operation_count: AtomicU64,
    pub error_count: AtomicU64,
    
    // 启动时间
    pub start_time: Instant,
}

impl LockFreeActorStats {
    pub fn new() -> Self {
        Self {
            data_packets_processed: AtomicU64::new(0),
            data_bytes_processed: AtomicU64::new(0),
            batch_operations: AtomicU64::new(0),
            total_batch_size: AtomicU64::new(0),
            max_batch_size: AtomicUsize::new(0),
            commands_processed: AtomicU64::new(0),
            command_latency_ns: AtomicU64::new(0),
            total_processing_time_ns: AtomicU64::new(0),
            operation_count: AtomicU64::new(0),
            error_count: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }
    
    pub fn record_data_batch(&self, batch_size: usize, total_bytes: usize) {
        self.data_packets_processed.fetch_add(batch_size as u64, Ordering::Relaxed);
        self.data_bytes_processed.fetch_add(total_bytes as u64, Ordering::Relaxed);
        self.batch_operations.fetch_add(1, Ordering::Relaxed);
        self.total_batch_size.fetch_add(batch_size as u64, Ordering::Relaxed);
        self.max_batch_size.fetch_max(batch_size, Ordering::Relaxed);
    }
    
    pub fn record_command(&self, latency_ns: u64) {
        self.commands_processed.fetch_add(1, Ordering::Relaxed);
        self.command_latency_ns.fetch_add(latency_ns, Ordering::Relaxed);
    }
    
    pub fn record_processing_time(&self, processing_time_ns: u64) {
        self.total_processing_time_ns.fetch_add(processing_time_ns, Ordering::Relaxed);
        self.operation_count.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }
    
    pub fn snapshot(&self) -> ActorStats {
        let batch_ops = self.batch_operations.load(Ordering::Relaxed);
        let total_batch_size = self.total_batch_size.load(Ordering::Relaxed);
        
        ActorStats {
            data_packets_processed: self.data_packets_processed.load(Ordering::Relaxed),
            commands_processed: self.commands_processed.load(Ordering::Relaxed),
            total_batches: batch_ops,
            average_batch_size: if batch_ops > 0 {
                total_batch_size as f64 / batch_ops as f64
            } else {
                0.0
            },
            processing_latency_ns: {
                let ops = self.operation_count.load(Ordering::Relaxed);
                if ops > 0 {
                    self.total_processing_time_ns.load(Ordering::Relaxed) / ops
                } else {
                    0
                }
            },
            uptime_seconds: self.start_time.elapsed().as_secs(),
            error_count: self.error_count.load(Ordering::Relaxed),
        }
    }
}

/// Phase 3.2.2 优化的双管道Actor
pub struct OptimizedActor {
    // 数据管道（高频）
    data_tx: FlumeSender<Packet>,
    data_rx: FlumeReceiver<Packet>,
    
    // 命令管道（低频）
    command_tx: FlumeSender<ActorCommand>,
    command_rx: FlumeReceiver<ActorCommand>,
    
    // 事件广播
    event_tx: FlumeSender<ActorEvent>,
    
    // 协议适配器
    protocol_adapter: FlumePoweredProtocolAdapter,
    
    // LockFree统计
    stats: Arc<LockFreeActorStats>,
    
    // 会话信息
    session_id: SessionId,
    
    // 批量处理配置
    max_batch_size: usize,
    batch_timeout_ms: u64,
}

impl OptimizedActor {
    /// 创建新的优化Actor
    pub fn new(
        session_id: SessionId,
        protocol_adapter: FlumePoweredProtocolAdapter,
        max_batch_size: usize,
        batch_timeout_ms: u64,
    ) -> (Self, FlumeReceiver<ActorEvent>, FlumeSender<Packet>, FlumeSender<ActorCommand>) {
        let (data_tx, data_rx) = flume::unbounded();
        let (command_tx, command_rx) = flume::unbounded();
        let (event_tx, event_rx) = flume::unbounded();
        
        let actor = Self {
            data_tx: data_tx.clone(),
            data_rx,
            command_tx: command_tx.clone(),
            command_rx,
            event_tx,
            protocol_adapter,
            stats: Arc::new(LockFreeActorStats::new()),
            session_id,
            max_batch_size,
            batch_timeout_ms,
        };
        
        (actor, event_rx, data_tx, command_tx)
    }
    
    /// 核心优化：双管道运行循环
    pub async fn run_flume_pipeline(mut self) -> Result<(), TransportError> {
        println!("🚀 启动优化Actor双管道处理 (会话: {})", self.session_id);
        
        // 启动数据处理管道
        let data_processor = {
            let data_rx = self.data_rx.clone();
            let event_tx = self.event_tx.clone();
            let stats = self.stats.clone();
            let max_batch = self.max_batch_size;
            
            tokio::spawn(async move {
                Self::run_data_pipeline(data_rx, event_tx, stats, max_batch).await
            })
        };
        
        // 启动命令处理管道
        let command_processor = {
            let command_rx = self.command_rx.clone();
            let event_tx = self.event_tx.clone();
            let stats = self.stats.clone();
            
            tokio::spawn(async move {
                Self::run_command_pipeline(command_rx, event_tx, stats).await
            })
        };
        
        // 等待任一管道完成
        tokio::select! {
            result = data_processor => {
                println!("📦 数据处理管道完成: {:?}", result);
            },
            result = command_processor => {
                println!("🎛️ 命令处理管道完成: {:?}", result);
            },
        }
        
        // 发送关闭事件
        let _ = self.event_tx.try_send(ActorEvent::Shutdown);
        
        Ok(())
    }
    
    /// 核心优化：专门的数据处理管道
    async fn run_data_pipeline(
        data_rx: FlumeReceiver<Packet>,
        event_tx: FlumeSender<ActorEvent>,
        stats: Arc<LockFreeActorStats>,
        max_batch_size: usize,
    ) {
        println!("📦 启动数据处理管道 (最大批次: {})", max_batch_size);
        
        while let Ok(first_packet) = data_rx.recv_async().await {
            let start_time = Instant::now();
            let mut batch = vec![first_packet];
            
            // 🚀 关键优化：批量收集数据包
            while batch.len() < max_batch_size {
                match data_rx.try_recv() {
                    Ok(packet) => batch.push(packet),
                    Err(_) => break,
                }
            }
            
            // 批量处理数据包
            let batch_size = batch.len();
            let total_bytes: usize = batch.iter().map(|p| p.payload.len()).sum();
            
            // 模拟数据处理（实际实现会调用具体的处理逻辑）
            Self::process_packet_batch(&batch).await;
            
            // 更新统计
            stats.record_data_batch(batch_size, total_bytes);
            let processing_time = start_time.elapsed().as_nanos() as u64;
            stats.record_processing_time(processing_time);
            
            // 发送事件
            let _ = event_tx.try_send(ActorEvent::BatchCompleted(batch_size));
            
            if batch_size > 1 {
                println!("📦 批量处理 {} 个数据包 ({} bytes, {:.2}μs)", 
                        batch_size, total_bytes, processing_time as f64 / 1000.0);
            }
        }
        
        println!("📦 数据处理管道退出");
    }
    
    /// 专门的命令处理管道
    async fn run_command_pipeline(
        command_rx: FlumeReceiver<ActorCommand>,
        event_tx: FlumeSender<ActorEvent>,
        stats: Arc<LockFreeActorStats>,
    ) {
        println!("🎛️ 启动命令处理管道");
        
        while let Ok(command) = command_rx.recv_async().await {
            let start_time = Instant::now();
            
            match command {
                ActorCommand::ProcessData(packet) => {
                    // 单个数据包处理
                    Self::process_single_packet(&packet).await;
                },
                ActorCommand::SendPacket(packet) => {
                    // 发送数据包
                    println!("📤 发送数据包: {} bytes", packet.payload.len());
                },
                ActorCommand::UpdateSession(session_id) => {
                    println!("🔄 更新会话: {}", session_id);
                },
                ActorCommand::GetStats => {
                    let stats_snapshot = stats.snapshot();
                    let _ = event_tx.try_send(ActorEvent::StatsReport(stats_snapshot));
                },
                ActorCommand::Shutdown => {
                    println!("🛑 收到关闭命令");
                    break;
                },
                ActorCommand::HealthCheck => {
                    println!("💓 健康检查通过");
                },
            }
            
            let latency = start_time.elapsed().as_nanos() as u64;
            stats.record_command(latency);
        }
        
        println!("🎛️ 命令处理管道退出");
    }
    
    /// 批量数据包处理
    async fn process_packet_batch(batch: &[Packet]) {
        // 模拟批量处理逻辑
        for (i, packet) in batch.iter().enumerate() {
            if i % 10 == 0 {
                tokio::task::yield_now().await;
            }
            // 实际处理逻辑会在这里
        }
    }
    
    /// 单个数据包处理
    async fn process_single_packet(packet: &Packet) {
        // 模拟单包处理
        tokio::task::yield_now().await;
    }
    
    /// 获取性能统计
    pub fn get_stats(&self) -> ActorStats {
        self.stats.snapshot()
    }
    
    /// 发送数据包到数据管道
    pub fn send_data(&self, packet: Packet) -> Result<(), TransportError> {
        self.data_tx.try_send(packet)
            .map_err(|_| TransportError::connection_error("Channel full: actor_data", false))
    }
    
    /// 发送命令到命令管道
    pub fn send_command(&self, command: ActorCommand) -> Result<(), TransportError> {
        self.command_tx.try_send(command)
            .map_err(|_| TransportError::connection_error("Channel full: actor_command", false))
    }
}

// 注意：FlumePoweredProtocolAdapter不支持Clone，我们将使用Arc包装

/// Actor管理器
pub struct ActorManager {
    actors: Vec<JoinHandle<Result<(), TransportError>>>,
    event_receivers: Vec<FlumeReceiver<ActorEvent>>,
    pub data_senders: Vec<FlumeSender<Packet>>,
    command_senders: Vec<FlumeSender<ActorCommand>>,
}

impl ActorManager {
    pub fn new() -> Self {
        Self {
            actors: Vec::new(),
            event_receivers: Vec::new(),
            data_senders: Vec::new(),
            command_senders: Vec::new(),
        }
    }
    
    /// 启动多个Actor
    pub async fn spawn_actors(
        &mut self,
        count: usize,
        max_batch_size: usize,
        batch_timeout_ms: u64,
    ) -> Result<(), TransportError> {
        for i in 0..count {
            let connection_info = ConnectionInfo {
                session_id: crate::SessionId::new(i as u64 + 1),
                local_addr: format!("127.0.0.1:{}", 8080 + i).parse().unwrap(),
                peer_addr: format!("127.0.0.1:{}", 9080 + i).parse().unwrap(),
                protocol: ProtocolType::Tcp,
                state: ConnectionState::Connected,
                established_at: std::time::SystemTime::now(),
                closed_at: None,
                last_activity: std::time::SystemTime::now(),
                packets_sent: 0,
                packets_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
            };
            
            let (protocol_adapter, _protocol_events) = 
                super::protocol_adapter_v2::FlumePoweredProtocolAdapter::new(crate::SessionId::new(i as u64 + 1), connection_info);
            
            let (actor, event_rx, data_tx, command_tx) = OptimizedActor::new(
                crate::SessionId::new(i as u64 + 1),
                protocol_adapter,
                max_batch_size,
                batch_timeout_ms,
            );
            
            let handle = tokio::spawn(actor.run_flume_pipeline());
            
            self.actors.push(handle);
            self.event_receivers.push(event_rx);
            self.data_senders.push(data_tx);
            self.command_senders.push(command_tx);
        }
        
        println!("🚀 已启动 {} 个优化Actor", count);
        Ok(())
    }
    
    /// 广播数据包到所有Actor
    pub fn broadcast_data(&self, packet: Packet) -> usize {
        let mut success_count = 0;
        for sender in &self.data_senders {
            if sender.try_send(packet.clone()).is_ok() {
                success_count += 1;
            }
        }
        success_count
    }
    
    /// 广播命令到所有Actor
    pub fn broadcast_command(&self, command: ActorCommand) -> usize {
        let mut success_count = 0;
        for sender in &self.command_senders {
            if sender.try_send(command.clone()).is_ok() {
                success_count += 1;
            }
        }
        success_count
    }
    
    /// 等待所有Actor完成
    pub async fn wait_all(&mut self) -> Vec<Result<Result<(), TransportError>, tokio::task::JoinError>> {
        let handles = std::mem::take(&mut self.actors);
        futures::future::join_all(handles).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::protocol_adapter_v2::create_test_packet;
    
    #[tokio::test]
    async fn test_optimized_actor_basic() {
        let connection_info = ConnectionInfo {
            session_id: crate::SessionId::new(1),
            local_addr: "127.0.0.1:8080".parse().unwrap(),
            peer_addr: "127.0.0.1:8081".parse().unwrap(),
            protocol: ProtocolType::Tcp,
            state: ConnectionState::Connected,
            established_at: std::time::SystemTime::now(),
            closed_at: None,
            last_activity: std::time::SystemTime::now(),
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        };
        
        let (protocol_adapter, _) = super::super::protocol_adapter_v2::FlumePoweredProtocolAdapter::new(1, connection_info);
        let (actor, _event_rx, data_tx, command_tx) = OptimizedActor::new(crate::SessionId::new(1), protocol_adapter, 32, 100);
        
        // 测试发送数据
        let packet = create_test_packet(1, 1024);
        assert!(data_tx.try_send(packet).is_ok());
        
        // 测试发送命令
        assert!(command_tx.try_send(ActorCommand::HealthCheck).is_ok());
        
        let stats = actor.get_stats();
        assert_eq!(stats.error_count, 0);
    }
    
    #[tokio::test]
    async fn test_actor_manager() {
        let mut manager = ActorManager::new();
        
        // 启动多个Actor
        assert!(manager.spawn_actors(3, 16, 50).await.is_ok());
        
        // 广播数据
        let packet = create_test_packet(1, 512);
        let sent_count = manager.broadcast_data(packet);
        assert_eq!(sent_count, 3);
        
        // 广播命令
        let sent_count = manager.broadcast_command(ActorCommand::HealthCheck);
        assert_eq!(sent_count, 3);
        
        // 等待一小段时间让处理完成
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // 发送关闭命令
        let _sent_count = manager.broadcast_command(ActorCommand::Shutdown);
        
        // 等待所有Actor完成
        let results = manager.wait_all().await;
        assert_eq!(results.len(), 3);
    }
} 