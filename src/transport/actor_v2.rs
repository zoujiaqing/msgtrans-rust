/// Phase 3.3: 前端Actor层完全迁移
/// 
/// 核心优化：
/// 1. 真实网络适配器集成
/// 2. 数据管道与命令管道分离  
/// 3. 批量数据处理
/// 4. Flume高性能通信

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use flume::{Sender as FlumeSender, Receiver as FlumeReceiver, unbounded as flume_unbounded};
use tokio::sync::{mpsc, Mutex};
use tracing::{info, debug, error};
use crate::{
    error::TransportError,
    packet::Packet,
    SessionId,
    command::{TransportCommand},
    protocol::adapter::ProtocolAdapter,
};

/// 🚀 Phase 3.3: 优化的Actor事件类型
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

/// 🚀 Phase 3.3: 优化的Actor命令类型
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

/// 🚀 Phase 3.3: LockFree Actor统计
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

/// 🚀 Phase 3.3: 优化的Actor实现 - 真实网络适配器集成
pub struct OptimizedActor<A: ProtocolAdapter> {
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
    
    /// 🌐 真实协议适配器（使用Arc<Mutex<>>共享）
    protocol_adapter: Arc<Mutex<A>>,
    
    /// 性能统计
    stats: Arc<LockFreeActorStats>,
    
    /// 批量处理配置
    max_batch_size: usize,
    
    /// 🌐 全局事件发送器（兼容现有系统）
    global_event_sender: tokio::sync::broadcast::Sender<crate::TransportEvent>,
}

impl<A: ProtocolAdapter> OptimizedActor<A> {
    /// 🚀 创建新的优化Actor（与真实网络适配器集成）
    pub fn new_with_real_adapter(
        session_id: SessionId,
        protocol_adapter: A,
        max_batch_size: usize,
        global_event_sender: tokio::sync::broadcast::Sender<crate::TransportEvent>,
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
            protocol_adapter: Arc::new(Mutex::new(protocol_adapter)),
            stats,
            max_batch_size,
            global_event_sender,
        };
        
        (actor, event_receiver, data_sender, command_sender)
    }
    
    /// 🚀 运行优化的双管道处理 - 真实网络适配器版本
    pub async fn run_dual_pipeline(mut self) -> Result<(), TransportError> 
    where 
        A: Send + 'static,
        A::Config: Send + 'static,
    {
        info!("🚀 启动优化Actor双管道处理 (会话: {})", self.session_id);
        
        // 启动命令适配任务
        let internal_cmd_sender = self.internal_command_sender.clone();
        let mut command_receiver = self.command_receiver;
        let session_id = self.session_id;
        
        let cmd_adapter_task = tokio::spawn(async move {
            info!("🎛️ 启动命令适配器 (会话: {})", session_id);
            while let Some(transport_cmd) = command_receiver.recv().await {
                match transport_cmd {
                    TransportCommand::Send { session_id: cmd_session_id, packet, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        // 发送到内部数据处理管道
                        if let Err(_) = internal_cmd_sender.send(ActorCommand::SendPacket(packet)) {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Internal channel closed", false)));
                            break;
                        } else {
                            // 发送成功，立即响应（数据会在数据处理管道中异步处理）
                            let _ = response_tx.send(Ok(()));
                        }
                    }
                    TransportCommand::Close { session_id: cmd_session_id, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        if let Err(_) = internal_cmd_sender.send(ActorCommand::Shutdown) {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Internal channel closed", false)));
                        } else {
                            let _ = response_tx.send(Ok(()));
                        }
                        break;
                    }
                    TransportCommand::GetStats { response_tx } => {
                        // 返回当前统计信息
                        let stats = crate::command::TransportStats::default(); // TODO: 从实际stats转换
                        let _ = response_tx.send(stats);
                    }
                    TransportCommand::GetConnectionInfo { session_id: cmd_session_id, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        // TODO: 返回实际连接信息
                        let info = crate::command::ConnectionInfo::default();
                        let _ = response_tx.send(Ok(info));
                    }
                    TransportCommand::Configure { .. } => {
                        debug!("🎛️ 配置命令暂不支持");
                    }
                    TransportCommand::GetActiveSessions { response_tx } => {
                        // 返回当前会话
                        let _ = response_tx.send(vec![session_id]);
                    }
                    TransportCommand::ForceDisconnect { session_id: cmd_session_id, reason: _, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        if let Err(_) = internal_cmd_sender.send(ActorCommand::Shutdown) {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Internal channel closed", false)));
                        } else {
                            let _ = response_tx.send(Ok(()));
                        }
                        break;
                    }
                    TransportCommand::PauseSession { session_id: cmd_session_id, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        debug!("🎛️ 暂停会话命令暂不支持");
                        let _ = response_tx.send(Ok(()));
                    }
                    TransportCommand::ResumeSession { session_id: cmd_session_id, response_tx } => {
                        if cmd_session_id != session_id {
                            let _ = response_tx.send(Err(crate::error::TransportError::connection_error("Invalid session", false)));
                            continue;
                        }
                        
                        debug!("🎛️ 恢复会话命令暂不支持");
                        let _ = response_tx.send(Ok(()));
                    }
                }
            }
            info!("🎛️ 命令适配器退出 (会话: {})", session_id);
        });
        
        // 启动数据处理管道
        let data_receiver = self.data_receiver.clone();
        let stats = self.stats.clone();
        let max_batch_size = self.max_batch_size;
        let protocol_adapter = self.protocol_adapter.clone();  // 克隆Arc
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
                        
                        let mut should_break = false;
                        for packet in batch.drain(..) {
                            // 🔧 在任务内部获取锁并发送
                            {
                                let mut adapter = protocol_adapter.lock().await;
                                
                                // 🔍 检查连接状态 - 如果连接已关闭，直接退出整个数据处理循环
                                if !adapter.is_connected() {
                                    debug!("📤 连接已关闭，停止数据处理管道");
                                    should_break = true;
                                    break; // 退出批次处理循环
                                }
                                
                                debug!("📤 发送数据包: {} bytes", packet.payload.len());
                                match adapter.send(packet.clone()).await {
                                    Ok(_) => {
                                        debug!("📤 发送成功: {} bytes", packet.payload.len());
                                        stats.record_packet_sent(packet.payload.len());
                                        
                                        // 发送全局事件（兼容现有系统）
                                        let transport_event = crate::TransportEvent::MessageSent {
                                            session_id,
                                            packet_id: packet.message_id,
                                        };
                                        let _ = global_event_sender.send(transport_event);
                                    }
                                    Err(e) => {
                                        // 🔍 简化错误处理，避免重复日志
                                        debug!("📤 发送失败（连接可能已关闭）: {:?}", e);
                                        stats.record_error();
                                        
                                        // 如果是连接关闭错误，停止处理
                                        if !adapter.is_connected() {
                                            debug!("📤 连接已关闭，停止数据处理管道");
                                            should_break = true;
                                            break;
                                        }
                                        
                                        // 发送错误事件
                                        let transport_event = crate::TransportEvent::TransportError {
                                            session_id: Some(session_id),
                                            error: TransportError::connection_error(format!("{:?}", e), false),
                                        };
                                        let _ = global_event_sender.send(transport_event);
                                    }
                                }
                            }
                        }
                        
                        // 如果连接已关闭，退出主循环
                        if should_break {
                            break;
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
        
        // 启动接收处理管道
        let stats = self.stats.clone();
        let event_sender = self.event_sender.clone();
        let global_event_sender = self.global_event_sender.clone();
        let protocol_adapter = self.protocol_adapter.clone();  // 克隆Arc用于接收
        let session_id = self.session_id;
        
        let recv_task = tokio::spawn(async move {
            info!("📥 启动接收处理管道");
            
            loop {
                // 🔧 从协议适配器接收数据
                let receive_result = {
                    let mut adapter = protocol_adapter.lock().await;
                    adapter.receive().await
                };
                
                match receive_result {
                    Ok(Some(packet)) => {
                        debug!("📥 接收到数据包: {} bytes", packet.payload.len());
                        stats.record_packet_received(packet.payload.len());
                        
                        // 发送内部Actor事件
                        let _ = event_sender.send(ActorEvent::PacketReceived { 
                            packet_id: packet.message_id, 
                            size: packet.payload.len() 
                        });
                        
                        // 🌐 发送全局事件（兼容现有系统）
                        let transport_event = crate::TransportEvent::MessageReceived {
                            session_id,
                            packet: packet.clone(),
                        };
                        
                        match global_event_sender.send(transport_event) {
                            Ok(_) => {
                                debug!("📥 成功发送MessageReceived事件 (会话: {})", session_id);
                            }
                            Err(e) => {
                                error!("📥 发送MessageReceived事件失败: {:?}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        debug!("📥 连接已关闭，无更多数据");
                        break;
                    }
                    Err(e) => {
                        error!("📥 接收数据时出错: {:?}", e);
                        stats.record_error();
                        
                        // 可以选择继续还是退出，这里选择短暂等待后继续
                        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        continue;
                    }
                }
            }
            
            info!("📥 接收处理管道退出");
            Ok::<(), TransportError>(())
        });
        
        // 启动命令处理管道
        let internal_command_receiver = self.internal_command_receiver;
        let event_sender = self.event_sender.clone();
        let global_event_sender = self.global_event_sender.clone();
        let data_sender = self.data_sender.clone();  // 🔧 修复：克隆data_sender
        let session_id = self.session_id;
        
        let command_task = tokio::spawn(async move {
            info!("🎛️ 启动命令处理管道");
            
            while let Ok(command) = internal_command_receiver.recv_async().await {
                match command {
                    ActorCommand::SendPacket(packet) => {
                        // 将数据包发送到数据处理管道
                        if let Err(_) = data_sender.send(packet) {
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
                        let transport_event = crate::TransportEvent::ConnectionClosed {
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
        let (cmd_adapter_result, data_result, recv_result, command_result) = 
            tokio::join!(cmd_adapter_task, data_task, recv_task, command_task);
        
        match (cmd_adapter_result, data_result, recv_result, command_result) {
            (Ok(()), Ok(Ok(())), Ok(Ok(())), Ok(Ok(()))) => {
                info!("✅ 优化Actor正常退出 (会话: {})", self.session_id);
                Ok(())
            }
            (cmd_res, data_res, recv_res, cmd_pipeline_res) => {
                error!("❌ 优化Actor异常退出 (会话: {}): cmd_adapter={:?}, data={:?}, recv={:?}, cmd_pipeline={:?}", 
                       self.session_id, cmd_res, data_res, recv_res, cmd_pipeline_res);
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

/// 🚀 Phase 3.3: 类型擦除的Actor管理器
pub struct ActorManager {
    /// 使用动态分发来管理不同类型的Actor
    actor_handles: Vec<tokio::task::JoinHandle<Result<(), TransportError>>>,
    stats: Arc<LockFreeActorStats>,
}

impl ActorManager {
    /// 创建新的ActorManager
    pub fn new() -> Self {
        Self {
            actor_handles: Vec::new(),
            stats: Arc::new(LockFreeActorStats::new()),
        }
    }
    
    /// 添加Actor（启动并管理）
    pub fn add_actor<A>(&mut self, actor: OptimizedActor<A>) 
    where 
        A: ProtocolAdapter + Send + 'static,
        A::Config: Send + 'static,
    {
        let handle = tokio::spawn(async move {
            actor.run_dual_pipeline().await
        });
        self.actor_handles.push(handle);
    }
    
    /// 并发运行所有Actor
    pub async fn run_all(self) -> Result<(), TransportError> {
        let results = futures::future::join_all(self.actor_handles).await;
        
        for (index, result) in results.into_iter().enumerate() {
            match result {
                Ok(Ok(())) => {
                    info!("✅ Actor {} 正常完成", index);
                }
                Ok(Err(e)) => {
                    error!("❌ Actor {} 运行错误: {:?}", index, e);
                }
                Err(e) => {
                    error!("❌ Actor {} 任务错误: {:?}", index, e);
                }
            }
        }
        
        info!("🏁 所有Actor已完成");
        Ok(())
    }
}
