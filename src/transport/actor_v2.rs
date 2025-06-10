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
use flume::{Sender as FlumeSender, Receiver as FlumeReceiver, unbounded as flume_unbounded, Receiver, Sender, unbounded};
use tokio::task::JoinHandle;
use tokio::sync::mpsc;
use tracing::{info, debug, error, warn};
use crate::{
    error::TransportError,
    packet::Packet,
    SessionId,
    command::{ConnectionInfo, ProtocolType, ConnectionState, TransportCommand},
};
use super::protocol_adapter_v2::{FlumePoweredProtocolAdapter, ProtocolEvent};

/// 🚀 Phase 3.2.2: 优化的Actor事件类型
#[derive(Debug, Clone)]
pub enum ActorEvent {
    /// 连接建立
    Connected,
    /// 数据包发送完成
    PacketSent { packet_id: u32, size: usize },
    /// 数据包接收完成  
    PacketReceived { packet_id: u32, size: usize },
    /// 批量处理完成
    BatchCompleted(usize),
    /// 错误发生
    Error { message: String },
    /// Actor关闭
    Shutdown,
    /// 健康检查
    HealthCheck,
}

/// 🚀 Phase 3.2.2: 优化的Actor命令类型
#[derive(Debug, Clone)]
pub enum ActorCommand {
    /// 发送数据包
    SendPacket(Packet),
    /// 获取统计信息
    GetStats,
    /// 健康检查
    HealthCheck,
    /// 关闭Actor
    Shutdown,
}

/// 🚀 Phase 3.2.2: LockFree Actor统计
#[derive(Debug, Default)]
pub struct LockFreeActorStats {
    /// 已发送数据包数
    pub packets_sent: AtomicU64,
    /// 已接收数据包数
    pub packets_received: AtomicU64,
    /// 发送字节数
    pub bytes_sent: AtomicU64,
    /// 接收字节数
    pub bytes_received: AtomicU64,
    /// 批量处理次数
    pub batch_operations: AtomicU64,
    /// 总处理的数据包数（批量）
    pub total_batch_packets: AtomicU64,
    /// 错误计数
    pub error_count: AtomicU64,
    /// 启动时间
    pub started_at: std::sync::Mutex<Option<Instant>>,
}

/// Actor统计快照
pub type ActorStats = LockFreeActorStats;

impl LockFreeActorStats {
    /// 创建新的统计实例
    pub fn new() -> Self {
        Self {
            started_at: std::sync::Mutex::new(Some(Instant::now())),
            ..Default::default()
        }
    }
    
    /// 记录数据包发送
    pub fn record_packet_sent(&self, size: usize) {
        self.packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size as u64, Ordering::Relaxed);
    }
    
    /// 记录数据包接收
    pub fn record_packet_received(&self, size: usize) {
        self.packets_received.fetch_add(1, Ordering::Relaxed);
        self.bytes_received.fetch_add(size as u64, Ordering::Relaxed);
    }
    
    /// 记录批量操作
    pub fn record_batch_operation(&self, packet_count: usize) {
        self.batch_operations.fetch_add(1, Ordering::Relaxed);
        self.total_batch_packets.fetch_add(packet_count as u64, Ordering::Relaxed);
    }
    
    /// 记录错误
    pub fn record_error(&self) {
        self.error_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 平均批次大小
    pub fn average_batch_size(&self) -> f64 {
        let batch_ops = self.batch_operations.load(Ordering::Relaxed);
        if batch_ops == 0 {
            0.0
        } else {
            self.total_batch_packets.load(Ordering::Relaxed) as f64 / batch_ops as f64
        }
    }
    
    /// 获取运行时间（秒）
    pub fn uptime_seconds(&self) -> u64 {
        self.started_at
            .lock()
            .unwrap()
            .as_ref()
            .map(|t| t.elapsed().as_secs())
            .unwrap_or(0)
    }
}

/// 🚀 Phase 3.2.2: 优化的Actor实现
pub struct OptimizedActor {
    /// 会话ID
    session_id: SessionId,
    
    /// 🔧 命令接收通道（兼容传统 mpsc）
    command_receiver: mpsc::Receiver<TransportCommand>,
    
    /// 📡 事件发送通道
    event_sender: FlumeSender<ActorEvent>,
    
    /// 🚀 内部高性能数据处理通道
    data_sender: FlumeSender<Packet>,
    data_receiver: FlumeReceiver<Packet>,
    
    /// 🚀 内部高性能命令处理通道  
    internal_command_sender: FlumeSender<ActorCommand>,
    internal_command_receiver: FlumeReceiver<ActorCommand>,
    
    /// 协议适配器
    protocol_adapter: FlumePoweredProtocolAdapter,
    
    /// 性能统计
    stats: Arc<LockFreeActorStats>,
    
    /// 批量处理配置
    max_batch_size: usize,
    batch_timeout_ms: u64,
    
    /// 🌐 全局事件发送器（兼容现有系统）
    global_event_sender: tokio::sync::broadcast::Sender<crate::Event>,
}

impl OptimizedActor {
    /// 🚀 创建新的优化Actor（兼容模式）
    pub fn new_compatible(
        session_id: SessionId,
        protocol_adapter: FlumePoweredProtocolAdapter,
        max_batch_size: usize,
        batch_timeout_ms: u64,
        global_event_sender: tokio::sync::broadcast::Sender<crate::Event>,
    ) -> (Self, FlumeReceiver<ActorEvent>, FlumeSender<Packet>, mpsc::Sender<TransportCommand>) {
        let (event_sender, event_receiver) = flume_unbounded();
        let (data_sender, data_receiver) = flume_unbounded();
        let (internal_command_sender, internal_command_receiver) = flume_unbounded();
        let (command_sender, command_receiver) = mpsc::channel(1024);
        
        let stats = Arc::new(LockFreeActorStats::new());
        
        let actor = Self {
            session_id,
            command_receiver,
            event_sender,
            data_sender: data_sender.clone(),
            data_receiver,
            internal_command_sender,
            internal_command_receiver,
            protocol_adapter,
            stats,
            max_batch_size,
            batch_timeout_ms,
            global_event_sender,
        };
        
        (actor, event_receiver, data_sender, command_sender)
    }
    
    /// 🚀 运行优化的双管道处理
    pub async fn run_dual_pipeline(mut self) -> Result<(), TransportError> {
        info!("🚀 启动优化Actor双管道处理 (会话: {})", self.session_id);
        
        // 启动命令适配任务
        let internal_cmd_sender = self.internal_command_sender.clone();
        let mut command_receiver = self.command_receiver;
        let session_id = self.session_id;
        
        let cmd_adapter_task = tokio::spawn(async move {
            info!("🎛️ 启动命令适配器 (会话: {})", session_id);
            while let Some(transport_cmd) = command_receiver.recv().await {
                let actor_cmd = match transport_cmd {
                    TransportCommand::Send { packet, .. } => ActorCommand::SendPacket(packet),
                    TransportCommand::Close { .. } => ActorCommand::Shutdown,
                    TransportCommand::GetStats { .. } => ActorCommand::GetStats,
                    _ => {
                        debug!("🎛️ 忽略未知命令: {:?}", transport_cmd);
                        continue;
                    },
                };
                
                if let Err(_) = internal_cmd_sender.send(actor_cmd) {
                    debug!("🎛️ 命令适配器：内部通道已关闭");
                    break;
                }
            }
            info!("🎛️ 命令适配器退出 (会话: {})", session_id);
        });
        
        // 启动数据处理管道
        let data_receiver = self.data_receiver.clone();
        let stats = self.stats.clone();
        let max_batch_size = self.max_batch_size;
        let protocol_adapter = self.protocol_adapter.clone();
        let event_sender = self.event_sender.clone();
        let global_event_sender = self.global_event_sender.clone();
        let session_id = self.session_id;
        
        let data_task = tokio::spawn(async move {
            info!("📦 启动数据处理管道 (最大批次: {})", max_batch_size);
            let mut batch = Vec::with_capacity(max_batch_size);
            
            loop {
                // 尝试收集一批数据包
                match data_receiver.recv_async().await {
                    Ok(packet) => {
                        batch.push(packet);
                        
                        // 尝试收集更多数据包到批次中
                        while batch.len() < max_batch_size {
                            match data_receiver.try_recv() {
                                Ok(packet) => batch.push(packet),
                                Err(_) => break, // 没有更多数据包，处理当前批次
                            }
                        }
                        
                        // 处理批次
                        let batch_size = batch.len();
                        debug!("📦 处理数据包批次: {} 个包", batch_size);
                        
                        for packet in batch.drain(..) {
                            // 发送数据包
                            debug!("📤 发送数据包: {} bytes", packet.payload.len());
                            
                            // 这里应该通过协议适配器发送
                            if let Err(e) = protocol_adapter.send_nowait(packet.clone()) {
                                error!("📤 发送失败: {:?}", e);
                                stats.record_error();
                                continue;
                            }
                            
                            stats.record_packet_sent(packet.payload.len());
                            
                            // 发送全局事件（兼容现有系统）
                            let transport_event = crate::Event::MessageSent {
                                session_id,
                                packet_id: packet.message_id,
                            };
                            let _ = global_event_sender.send(transport_event);
                        }
                        
                        stats.record_batch_operation(batch_size);
                        let _ = event_sender.send(ActorEvent::BatchCompleted(batch_size));
                    }
                    Err(_) => {
                        debug!("📦 数据处理管道：接收通道已关闭");
                        break;
                    }
                }
            }
            
            info!("📦 数据处理管道退出");
            Ok::<(), TransportError>(())
        });
        
        // 启动命令处理管道
        let internal_command_receiver = self.internal_command_receiver;
        let event_sender = self.event_sender.clone();
        let global_event_sender = self.global_event_sender.clone();
        let session_id = self.session_id;
        
        let command_task = tokio::spawn(async move {
            info!("🎛️ 启动命令处理管道");
            
            while let Ok(command) = internal_command_receiver.recv_async().await {
                match command {
                    ActorCommand::SendPacket(packet) => {
                        // 将数据包发送到数据处理管道
                        if let Err(_) = self.data_sender.send(packet) {
                            error!("🎛️ 无法发送到数据管道：通道已关闭");
                            break;
                        }
                    }
                    ActorCommand::GetStats => {
                        debug!("🎛️ 处理统计查询");
                        // 可以通过事件返回统计信息
                    }
                    ActorCommand::Shutdown => {
                        info!("🛑 收到关闭命令");
                        let _ = event_sender.send(ActorEvent::Shutdown);
                        
                        // 发送全局关闭事件
                        let transport_event = crate::Event::ConnectionClosed {
                            session_id,
                            reason: crate::CloseReason::Normal,
                        };
                        let _ = global_event_sender.send(transport_event);
                        break;
                    }
                    ActorCommand::HealthCheck => {
                        debug!("💊 健康检查");
                        let _ = event_sender.send(ActorEvent::HealthCheck);
                    }
                }
            }
            
            info!("🎛️ 命令处理管道退出");
            Ok::<(), TransportError>(())
        });
        
        // 等待所有任务完成
        let (cmd_adapter_result, data_result, command_result) = 
            tokio::join!(cmd_adapter_task, data_task, command_task);
        
        match (cmd_adapter_result, data_result, command_result) {
            (Ok(()), Ok(Ok(())), Ok(Ok(()))) => {
                info!("✅ 优化Actor正常退出 (会话: {})", self.session_id);
                Ok(())
            }
            (cmd_res, data_res, cmd_pipeline_res) => {
                error!("❌ 优化Actor异常退出 (会话: {}): cmd_adapter={:?}, data={:?}, cmd_pipeline={:?}", 
                       self.session_id, cmd_res, data_res, cmd_pipeline_res);
                Err(TransportError::connection_error("Actor pipeline failed", false))
            }
        }
    }

    /// 🔧 兼容方法：模拟传统Actor的运行
    pub async fn run_flume_pipeline(self) -> Result<(), TransportError> {
        self.run_dual_pipeline().await
    }

    /// 获取统计信息
    pub fn get_stats(&self) -> Arc<LockFreeActorStats> {
        self.stats.clone()
    }
}

/// ActorManager - 管理多个优化Actor
pub struct ActorManager {
    actors: Vec<OptimizedActor>,
    stats: Arc<LockFreeActorStats>,
}

impl ActorManager {
    /// 创建新的ActorManager
    pub fn new() -> Self {
        Self {
            actors: Vec::new(),
            stats: Arc::new(LockFreeActorStats::new()),
        }
    }
    
    /// 添加Actor
    pub fn add_actor(&mut self, actor: OptimizedActor) {
        self.actors.push(actor);
    }
    
    /// 并发运行所有Actor
    pub async fn run_all(self) -> Result<(), TransportError> {
        let mut handles = Vec::new();
        
        for actor in self.actors {
            let handle = tokio::spawn(async move {
                actor.run_dual_pipeline().await
            });
            handles.push(handle);
        }
        
        // 等待所有Actor完成
        for handle in handles {
            handle.await.map_err(|e| TransportError::connection_error(&format!("Actor join error: {}", e), false))??;
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    
    #[tokio::test]
    async fn test_optimized_actor_creation() {
        // 创建测试依赖
        let now = std::time::SystemTime::now();
        let connection_info = ConnectionInfo {
            session_id: SessionId::new(1),
            local_addr: "127.0.0.1:8080".parse().unwrap(),
            peer_addr: "127.0.0.1:8081".parse().unwrap(),
            protocol: ProtocolType::Tcp,
            state: ConnectionState::Connected,
            established_at: now,
            closed_at: None,
            last_activity: now,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        };
        
        let (protocol_adapter, _) = FlumePoweredProtocolAdapter::new(SessionId::new(1), connection_info);
        let (global_event_sender, _) = tokio::sync::broadcast::channel(1024);
        
        // 创建OptimizedActor
        let (actor, _event_rx, _data_tx, _command_tx) = OptimizedActor::new_compatible(
            SessionId::new(1),
            protocol_adapter,
            32,
            100,
            global_event_sender,
        );
        
        // 验证统计初始状态
        let stats = actor.get_stats();
        assert_eq!(stats.packets_sent.load(Ordering::Relaxed), 0);
        assert_eq!(stats.packets_received.load(Ordering::Relaxed), 0);
        assert!(stats.uptime_seconds() >= 0);
    }
    
    #[tokio::test]
    async fn test_actor_manager() {
        let mut manager = ActorManager::new();
        
        for i in 0..3 {
            let now = std::time::SystemTime::now();
            let connection_info = ConnectionInfo {
                session_id: SessionId::new(i as u64),
                local_addr: "127.0.0.1:8080".parse().unwrap(),
                peer_addr: "127.0.0.1:8081".parse().unwrap(),
                protocol: ProtocolType::Tcp,
                state: ConnectionState::Connected,
                established_at: now,
                closed_at: None,
                last_activity: now,
                packets_sent: 0,
                packets_received: 0,
                bytes_sent: 0,
                bytes_received: 0,
            };
            
            let (protocol_adapter, _) = FlumePoweredProtocolAdapter::new(SessionId::new(i as u64), connection_info);
            let (global_event_sender, _) = tokio::sync::broadcast::channel(1024);
            
            let (actor, _event_rx, _data_tx, _command_tx) = OptimizedActor::new_compatible(
                SessionId::new(i as u64),
                protocol_adapter,
                16,
                50,
                global_event_sender,
            );
            
            manager.add_actor(actor);
        }
        
        // 验证manager创建成功
        assert_eq!(manager.actors.len(), 3);
    }
} 