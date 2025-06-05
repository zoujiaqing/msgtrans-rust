use std::collections::HashMap;
use tokio::sync::{mpsc, broadcast, Mutex};
use std::sync::Arc;
use super::{
    SessionId, 
    adapter::{ProtocolAdapter, AdapterStats},
    command::{TransportCommand, TransportStats, ConnectionInfo, ProtocolType, ConnectionState},
    event::TransportEvent,
    error::TransportError,
    packet::UnifiedPacket,
    CloseReason,
};

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
    config: A::Config,
    /// 运行状态
    state: ActorState,
    /// 统计信息
    stats: TransportStats,
    /// Actor句柄引用计数
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
                    match result {
                        Ok(Some(packet)) => {
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
                    let _ = response_tx.send(Err(TransportError::InvalidSession));
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
                let _ = response_tx.send(Err(TransportError::InvalidSession));
            }
            
            TransportCommand::GetStats { response_tx } => {
                let _ = response_tx.send(self.stats.clone());
            }
            
            TransportCommand::GetConnectionInfo { session_id, response_tx } => {
                if session_id == self.session_id {
                    let info = self.adapter.connection_info();
                    let _ = response_tx.send(Ok(info));
                } else {
                    let _ = response_tx.send(Err(TransportError::InvalidSession));
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
                let _ = response_tx.send(Err(TransportError::InvalidSession));
            }
            
            TransportCommand::PauseSession { session_id, response_tx } => {
                if session_id == self.session_id {
                    // 暂停逻辑 - 可以暂停数据处理
                    tracing::info!("Pausing session {}", session_id);
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err(TransportError::InvalidSession));
                }
            }
            
            TransportCommand::ResumeSession { session_id, response_tx } => {
                if session_id == self.session_id {
                    // 恢复逻辑
                    tracing::info!("Resuming session {}", session_id);
                    let _ = response_tx.send(Ok(()));
                } else {
                    let _ = response_tx.send(Err(TransportError::InvalidSession));
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
    async fn handle_received_packet(&mut self, packet: UnifiedPacket) {
        let packet_size = packet.payload.len();
        self.stats.record_packet_received(packet_size);
        
        let _ = self.event_tx.send(TransportEvent::PacketReceived {
            session_id: self.session_id,
            packet,
        });
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
            reason: CloseReason::Normal,
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
    pub async fn send_packet(&self, packet: UnifiedPacket) -> Result<(), TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::Send {
            session_id: self.session_id,
            packet,
            response_tx,
        }).await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)?
    }
    
    /// 关闭连接
    pub async fn close(&self) -> Result<(), TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::Close {
            session_id: self.session_id,
            response_tx,
        }).await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)?
    }
    
    /// 获取统计信息
    pub async fn stats(&self) -> Result<TransportStats, TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::GetStats { response_tx })
            .await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)
    }
    
    /// 获取连接信息
    pub async fn connection_info(&self) -> Result<ConnectionInfo, TransportError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.send(TransportCommand::GetConnectionInfo {
            session_id: self.session_id,
            response_tx,
        }).await.map_err(|_| TransportError::ChannelClosed)?;
        
        response_rx.await.map_err(|_| TransportError::ChannelClosed)?
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

/// Actor管理器
/// 
/// 管理多个Actor的生命周期
pub struct ActorManager {
    /// 活跃的Actor句柄
    actors: Arc<Mutex<HashMap<SessionId, ActorHandle>>>,
    /// 全局事件发送器
    global_event_tx: broadcast::Sender<TransportEvent>,
}

impl ActorManager {
    /// 创建新的Actor管理器
    pub fn new() -> Self {
        let (global_event_tx, _) = broadcast::channel(1024);
        
        Self {
            actors: Arc::new(Mutex::new(HashMap::new())),
            global_event_tx,
        }
    }
    
    /// 添加Actor
    pub async fn add_actor(&self, session_id: SessionId, handle: ActorHandle) {
        let mut actors = self.actors.lock().await;
        actors.insert(session_id, handle);
    }
    
    /// 移除Actor
    pub async fn remove_actor(&self, session_id: &SessionId) -> Option<ActorHandle> {
        let mut actors = self.actors.lock().await;
        actors.remove(session_id)
    }
    
    /// 获取Actor句柄
    pub async fn get_actor(&self, session_id: &SessionId) -> Option<ActorHandle> {
        let actors = self.actors.lock().await;
        actors.get(session_id).cloned()
    }
    
    /// 获取所有活跃会话ID
    pub async fn active_sessions(&self) -> Vec<SessionId> {
        let actors = self.actors.lock().await;
        actors.keys().copied().collect()
    }
    
    /// 广播事件到所有Actor
    pub async fn broadcast_event(&self, event: TransportEvent) {
        let _ = self.global_event_tx.send(event);
    }
    
    /// 获取全局事件接收器
    pub fn global_events(&self) -> broadcast::Receiver<TransportEvent> {
        self.global_event_tx.subscribe()
    }
}

impl Default for ActorManager {
    fn default() -> Self {
        Self::new()
    }
} 