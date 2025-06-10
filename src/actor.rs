use tokio::sync::{mpsc, broadcast, Mutex};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use flume::{unbounded as flume_unbounded, Receiver as FlumeReceiver, Sender as FlumeSender};

use crate::{
    SessionId, 
    protocol::ProtocolAdapter,
    command::{TransportCommand, TransportStats, ConnectionInfo},
    event::TransportEvent,
    error::TransportError,
    packet::Packet,
    transport::lockfree_enhanced::LockFreeHashMap,
};

/// 🚀 Phase 2: Actor管理命令 (异步高性能)
#[derive(Debug)]
pub enum ActorManagerCommand {
    AddActor(SessionId, ActorHandle),
    RemoveActor(SessionId),
    BroadcastEvent(TransportEvent),
    GetStats,
    Shutdown,
}

/// Actor状态枚举
#[derive(Debug, Clone, PartialEq)]
pub enum ActorState {
    /// 初始化中
    Initializing,
    /// 运行中
    Running,
    /// 正在停止
    Stopping,
    /// 已停止
    Stopped,
    /// 错误状态
    Error(String),
}

/// 泛型传输Actor
/// 
/// 这是统一架构的核心组件，负责管理单个连接的生命周期
pub struct GenericActor<A: ProtocolAdapter> {
    /// 协议适配器
    adapter: A,
    /// 会话ID
    session_id: SessionId,
    /// 命令接收器
    command_rx: mpsc::Receiver<TransportCommand>,
    /// 事件发送器
    event_tx: broadcast::Sender<TransportEvent>,
    /// 配置
    #[allow(dead_code)]
    config: A::Config,
    /// 运行状态
    state: ActorState,
    /// 统计信息
    stats: TransportStats,
    /// Actor句柄引用计数
    #[allow(dead_code)]
    handle_count: Arc<Mutex<usize>>,
}

impl<A: ProtocolAdapter> GenericActor<A> {
    /// 创建新的Actor
    pub fn new(
        adapter: A,
        session_id: SessionId,
        command_rx: mpsc::Receiver<TransportCommand>,
        event_tx: broadcast::Sender<TransportEvent>,
        config: A::Config,
    ) -> Self {
        Self {
            adapter,
            session_id,
            command_rx,
            event_tx,
            config,
            state: ActorState::Initializing,
            stats: TransportStats::new(),
            handle_count: Arc::new(Mutex::new(0)),
        }
    }
    
    /// 获取Actor状态
    pub fn state(&self) -> &ActorState {
        &self.state
    }
    
    /// 获取会话ID
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> &TransportStats {
        &self.stats
    }
    
    /// Actor主循环
    /// 
    /// 这是Actor的核心逻辑，处理命令和适配器事件
    pub async fn run(mut self) -> Result<(), TransportError> {
        self.state = ActorState::Running;
        tracing::info!("Actor {} started", self.session_id);
        
        // 发送连接建立事件
        let connection_info = self.adapter.connection_info();
        self.stats.record_connection_opened();
        
        let _ = self.event_tx.send(TransportEvent::ConnectionEstablished {
            session_id: self.session_id,
            info: connection_info,
        });
        
        // 主事件循环
        loop {
            tokio::select! {
                // 处理命令
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(command) => {
                            if let Err(e) = self.handle_command(command).await {
                                tracing::error!("Command handling error: {:?}", e);
                                if matches!(e, CommandHandlingResult::Stop) {
                                    break;
                                }
                            }
                        }
                        None => {
                            tracing::info!("Command channel closed for session {}", self.session_id);
                            break;
                        }
                    }
                }
                
                // 接收数据
                result = self.adapter.receive() => {
                    tracing::debug!("🔍 Actor {} 调用adapter.receive()结果: {:?}", self.session_id, 
                                   match &result { 
                                       Ok(Some(_)) => "收到数据包", 
                                       Ok(None) => "连接关闭", 
                                       Err(_) => "错误" 
                                   });
                    
                    match result {
                        Ok(Some(packet)) => {
                            tracing::info!("🔍 Actor {} 成功接收数据包: 类型{:?}, ID{}", 
                                          self.session_id, packet.packet_type, packet.message_id);
                            self.handle_received_packet(packet).await;
                        }
                        Ok(None) => {
                            tracing::info!("Connection closed by peer for session {}", self.session_id);
                            break;
                        }
                        Err(e) => {
                            self.handle_adapter_error(e).await;
                            break;
                        }
                    }
                }
                
                // 定期健康检查
                _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                    if !self.adapter.is_connected() {
                        tracing::warn!("Connection health check failed for session {}", self.session_id);
                        break;
                    }
                }
            }
        }
        
        // 清理资源
        self.cleanup().await;
        
        Ok(())
    }
    
    /// 处理命令
    async fn handle_command(&mut self, command: TransportCommand) -> Result<(), CommandHandlingResult> {
        match command {
            TransportCommand::Send { session_id, packet, response_tx } => {
                if session_id != self.session_id {
                    let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
                    return Ok(());
                }
                
                let packet_size = packet.payload.len();
                let result = self.adapter.send(packet).await.map_err(Into::into);
                
                if result.is_ok() {
                    self.stats.record_packet_sent(packet_size);
                } else {
                    self.stats.record_error();
                }
                
                let _ = response_tx.send(result);
            }
            
            TransportCommand::Close { session_id, response_tx } => {
                if session_id == self.session_id {
                    let result = self.adapter.close().await.map_err(Into::into);
                    let _ = response_tx.send(result);
                    return Err(CommandHandlingResult::Stop);
                }
                let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
            }
            
            TransportCommand::GetStats { response_tx } => {
                let _ = response_tx.send(self.stats.clone());
            }
            
            TransportCommand::GetConnectionInfo { session_id, response_tx } => {
                if session_id == self.session_id {
                    let info = self.adapter.connection_info();
                    let _ = response_tx.send(Ok(info));
                } else {
                    let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
                }
            }
            
            TransportCommand::GetActiveSessions { response_tx } => {
                let _ = response_tx.send(vec![self.session_id]);
            }
            
            TransportCommand::ForceDisconnect { session_id, reason, response_tx } => {
                if session_id == self.session_id {
                    tracing::warn!("Force disconnecting session {}: {}", session_id, reason);
                    let result = self.adapter.close().await.map_err(Into::into);
                    let _ = response_tx.send(result);
                    return Err(CommandHandlingResult::Stop);
                }
                let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
            }
            
            TransportCommand::PauseSession { session_id, response_tx } => {
                if session_id == self.session_id {
                    // 暂停逻辑 - 可以暂停数据处理
                    tracing::info!("Pausing session {}", session_id);
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
                }
            }
            
            TransportCommand::ResumeSession { session_id, response_tx } => {
                if session_id == self.session_id {
                    // 恢复逻辑
                    tracing::info!("Resuming session {}", session_id);
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err(TransportError::connection_error("Invalid session", false)));
                }
            }
            
            TransportCommand::Configure { config: _, response_tx } => {
                // 配置更新逻辑 - 暂时只返回成功
                let _ = response_tx.send(Ok(()));
            }
        }
        
        Ok(())
    }
    
    /// 处理接收到的数据包
    async fn handle_received_packet(&mut self, packet: Packet) {
        let packet_size = packet.payload.len();
        self.stats.record_packet_received(packet_size);
        
        tracing::info!("🔍 Actor {} 发送MessageReceived事件到全局事件流", self.session_id);
        
        let event = TransportEvent::MessageReceived {
            session_id: self.session_id,
            packet,
        };
        
        match self.event_tx.send(event) {
            Ok(receiver_count) => {
                tracing::info!("🔍 MessageReceived事件发送成功，有{}个接收者", receiver_count);
            }
            Err(e) => {
                tracing::error!("🔍 MessageReceived事件发送失败: {:?}", e);
            }
        }
    }
    
    /// 处理适配器错误
    async fn handle_adapter_error(&mut self, error: A::Error) {
        self.stats.record_error();
        
        let transport_error: TransportError = error.into();
        let _ = self.event_tx.send(TransportEvent::TransportError {
            session_id: Some(self.session_id),
            error: transport_error,
        });
    }
    
    /// 清理资源
    async fn cleanup(&mut self) {
        self.state = ActorState::Stopping;
        
        // 尝试优雅关闭适配器
        if let Err(e) = self.adapter.close().await {
            tracing::warn!("Error closing adapter for session {}: {:?}", self.session_id, e);
        }
        
        // 更新统计信息
        self.stats.record_connection_closed();
        
        // 发送连接关闭事件
        let _ = self.event_tx.send(TransportEvent::ConnectionClosed {
            session_id: self.session_id,
            reason: crate::CloseReason::Normal,
        });
        
        self.state = ActorState::Stopped;
        tracing::info!("Actor {} stopped", self.session_id);
    }
}

/// 命令处理结果
#[derive(Debug)]
enum CommandHandlingResult {
    Stop,
}

impl std::fmt::Display for CommandHandlingResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommandHandlingResult::Stop => write!(f, "Stop"),
        }
    }
}

impl std::error::Error for CommandHandlingResult {}

/// Actor句柄
/// 
/// 用于与Actor通信的轻量级句柄
#[derive(Debug)]
pub struct ActorHandle {
    /// 命令发送器
    command_tx: mpsc::Sender<TransportCommand>,
    /// 事件接收器
    event_rx: broadcast::Receiver<TransportEvent>,
    /// 会话ID
    session_id: SessionId,
    /// 句柄引用计数
    handle_count: Arc<Mutex<usize>>,
}

impl ActorHandle {
    /// 创建新的Actor句柄
    pub fn new(
        command_tx: mpsc::Sender<TransportCommand>,
        event_rx: broadcast::Receiver<TransportEvent>,
        session_id: SessionId,
        handle_count: Arc<Mutex<usize>>,
    ) -> Self {
        Self {
            command_tx,
            event_rx,
            session_id,
            handle_count,
        }
    }
    
    /// 获取会话ID
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    
    /// 发送数据包
    pub async fn send_packet(&self, packet: Packet) -> Result<(), TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::Send {
            session_id: self.session_id,
            packet,
            response_tx,
        }).await.map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await.map_err(|_| TransportError::connection_error("Channel closed", false))?
    }
    
    /// 关闭连接
    pub async fn close(&self) -> Result<(), TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::Close {
            session_id: self.session_id,
            response_tx,
        }).await.map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await.map_err(|_| TransportError::connection_error("Channel closed", false))?
    }
    
    /// 获取统计信息
    pub async fn stats(&self) -> Result<TransportStats, TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::GetStats { response_tx })
            .await.map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await.map_err(|_| TransportError::connection_error("Channel closed", false))
    }
    
    /// 获取连接信息
    pub async fn connection_info(&self) -> Result<ConnectionInfo, TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::GetConnectionInfo {
            session_id: self.session_id,
            response_tx,
        }).await.map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await.map_err(|_| TransportError::connection_error("Channel closed", false))?
    }
    
    /// 创建事件接收器
    pub fn events(&self) -> broadcast::Receiver<TransportEvent> {
        self.event_rx.resubscribe()
    }
}

impl Clone for ActorHandle {
    fn clone(&self) -> Self {
        // 增加引用计数
        if let Ok(mut count) = self.handle_count.try_lock() {
            *count += 1;
        }
        
        Self {
            command_tx: self.command_tx.clone(),
            event_rx: self.event_rx.resubscribe(),
            session_id: self.session_id,
            handle_count: self.handle_count.clone(),
        }
    }
}

impl Drop for ActorHandle {
    fn drop(&mut self) {
        // 减少引用计数
        if let Ok(mut count) = self.handle_count.try_lock() {
            if *count > 0 {
                *count -= 1;
            }
        }
    }
}

/// 🚀 Phase 2 迁移：混合架构Actor管理器
/// 
/// 使用 LockFree + Flume 混合架构管理多个Actor的生命周期
pub struct ActorManager {
    /// ✅ Phase 2: LockFree Actor句柄存储 (替代 Arc<Mutex<HashMap>>)
    actors: Arc<LockFreeHashMap<SessionId, ActorHandle>>,
    
    /// 🔧 Phase 2: Flume 异步命令通道 (替代直接操作)
    command_tx: FlumeSender<ActorManagerCommand>,
    command_rx: Option<FlumeReceiver<ActorManagerCommand>>,
    
    /// 全局事件发送器 (保持 Tokio 用于生态集成)
    pub(crate) global_event_tx: broadcast::Sender<TransportEvent>,
    
    /// 统计信息
    stats: Arc<ActorManagerStats>,
}

/// Actor管理器统计
#[derive(Debug, Default)]
pub struct ActorManagerStats {
    pub actors_added: AtomicU64,
    pub actors_removed: AtomicU64,
    pub events_broadcasted: AtomicU64,
    pub commands_processed: AtomicU64,
}

impl ActorManager {
    /// 🚀 Phase 2: 创建新的混合架构Actor管理器
    pub fn new() -> Self {
        let (global_event_tx, _) = broadcast::channel(1024);
        let (command_tx, command_rx) = flume_unbounded();
        
        Self {
            /// ✅ Phase 2: LockFree Actor存储
            actors: Arc::new(LockFreeHashMap::new()),
            
            /// 🔧 Phase 2: Flume 异步命令通道
            command_tx,
            command_rx: Some(command_rx),
            
            global_event_tx,
            stats: Arc::new(ActorManagerStats::default()),
        }
    }
    
    /// 🚀 Phase 2: 添加Actor (LockFree + Flume)
    pub async fn add_actor(&self, session_id: SessionId, handle: ActorHandle) {
        // 直接使用 LockFree 同步插入，无需异步
        if let Err(e) = self.actors.insert(session_id, handle) {
            tracing::error!("❌ 添加Actor失败 {}: {:?}", session_id, e);
        } else {
            self.stats.actors_added.fetch_add(1, Ordering::Relaxed);
            tracing::debug!("✅ Actor已添加: {}", session_id);
        }
    }
    
    /// 🚀 Phase 2: 移除Actor (LockFree)
    pub async fn remove_actor(&self, session_id: &SessionId) -> Option<ActorHandle> {
        match self.actors.remove(session_id) {
            Ok(handle) => {
                self.stats.actors_removed.fetch_add(1, Ordering::Relaxed);
                tracing::debug!("✅ Actor已移除: {}", session_id);
                handle
            },
            Err(e) => {
                tracing::warn!("⚠️ 移除Actor失败 {}: {:?}", session_id, e);
                None
            }
        }
    }
    
    /// 🚀 Phase 2: 获取Actor句柄 (LockFree wait-free读取)
    pub async fn get_actor(&self, session_id: &SessionId) -> Option<ActorHandle> {
        self.actors.get(session_id)
    }
    
    /// 🚀 Phase 2: 获取所有活跃会话ID (LockFree)
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        match self.actors.keys() {
            Ok(keys) => keys,
            Err(e) => {
                tracing::error!("❌ 获取活跃会话失败: {:?}", e);
                Vec::new()
            }
        }
    }
    
    /// 🚀 Phase 2: 广播事件到所有Actor (保持 Tokio 生态)
    pub async fn broadcast_event(&self, event: TransportEvent) {
        if let Err(e) = self.global_event_tx.send(event) {
            tracing::warn!("⚠️ 广播事件失败: {:?}", e);
        } else {
            self.stats.events_broadcasted.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// 获取全局事件接收器 (保持 Tokio 生态)
    pub fn global_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.global_event_tx.subscribe()
    }
    
    /// 🚀 Phase 2: 获取Actor管理器统计信息
    pub fn get_stats(&self) -> (u64, u64, u64, u64) {
        let actors_added = self.stats.actors_added.load(Ordering::Relaxed);
        let actors_removed = self.stats.actors_removed.load(Ordering::Relaxed);
        let events_broadcasted = self.stats.events_broadcasted.load(Ordering::Relaxed);
        let commands_processed = self.stats.commands_processed.load(Ordering::Relaxed);
        
        (actors_added, actors_removed, events_broadcasted, commands_processed)
    }
    
    /// 🚀 Phase 2: 获取当前活跃Actor数量 (LockFree)
    pub async fn actor_count(&self) -> usize {
        self.actors.len()
    }
}

impl Default for ActorManager {
    fn default() -> Self {
        Self::new()
    }
} 