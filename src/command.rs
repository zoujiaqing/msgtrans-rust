use tokio::sync::oneshot;
use crate::{SessionId, error::TransportError, packet::Packet};

/// 传输层命令的统一抽象
#[derive(Debug)]
pub enum TransportCommand {
    /// 发送数据包
    Send {
        session_id: SessionId,
        packet: Packet,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 关闭连接
    Close {
        session_id: SessionId,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 配置更新
    Configure {
        config: ConfigUpdate,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 获取统计信息
    GetStats {
        response_tx: oneshot::Sender<TransportStats>,
    },
    
    /// 获取连接信息
    GetConnectionInfo {
        session_id: SessionId,
        response_tx: oneshot::Sender<Result<ConnectionInfo, TransportError>>,
    },
    
    /// 获取所有活跃会话
    GetActiveSessions {
        response_tx: oneshot::Sender<Vec<SessionId>>,
    },
    
    /// 强制断开会话
    ForceDisconnect {
        session_id: SessionId,
        reason: String,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 暂停会话
    PauseSession {
        session_id: SessionId,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
    
    /// 恢复会话
    ResumeSession {
        session_id: SessionId,
        response_tx: oneshot::Sender<Result<(), TransportError>>,
    },
}

/// 配置更新类型
#[derive(Debug)]
pub enum ConfigUpdate {
    /// 设置缓冲区大小
    BufferSize(usize),
    /// 设置超时时间
    Timeout(std::time::Duration),
    /// 设置最大连接数
    MaxConnections(usize),
    /// 协议特定配置
    Protocol(Box<dyn std::any::Any + Send>),
}

/// 传输统计信息
#[derive(Debug, Clone)]
pub struct TransportStats {
    /// 总发送的数据包数量
    pub packets_sent: u64,
    /// 总接收的数据包数量
    pub packets_received: u64,
    /// 总发送的字节数
    pub bytes_sent: u64,
    /// 总接收的字节数
    pub bytes_received: u64,
    /// 活跃连接数
    pub active_connections: u64,
    /// 总连接数（历史）
    pub total_connections: u64,
    /// 错误计数
    pub errors: u64,
    /// 启动时间
    pub start_time: std::time::SystemTime,
    /// 最后活动时间
    pub last_activity: std::time::SystemTime,
}

impl Default for TransportStats {
    fn default() -> Self {
        let now = std::time::SystemTime::now();
        Self {
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            active_connections: 0,
            total_connections: 0,
            errors: 0,
            start_time: now,
            last_activity: now,
        }
    }
}

impl TransportStats {
    pub fn new() -> Self {
        Default::default()
    }
    
    pub fn update_activity(&mut self) {
        self.last_activity = std::time::SystemTime::now();
    }
    
    pub fn record_packet_sent(&mut self, size: usize) {
        self.packets_sent += 1;
        self.bytes_sent += size as u64;
        self.update_activity();
    }
    
    pub fn record_packet_received(&mut self, size: usize) {
        self.packets_received += 1;
        self.bytes_received += size as u64;
        self.update_activity();
    }
    
    pub fn record_connection_opened(&mut self) {
        self.active_connections += 1;
        self.total_connections += 1;
        self.update_activity();
    }
    
    pub fn record_connection_closed(&mut self) {
        if self.active_connections > 0 {
            self.active_connections -= 1;
        }
        self.update_activity();
    }
    
    pub fn record_error(&mut self) {
        self.errors += 1;
        self.update_activity();
    }
    
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed().unwrap_or_default()
    }
    
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_activity.elapsed().unwrap_or_default()
    }
}

/// 连接信息
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// 会话ID
    pub session_id: SessionId,
    /// 本地地址
    pub local_addr: std::net::SocketAddr,
    /// 远程地址
    pub peer_addr: std::net::SocketAddr,
    /// 协议类型
    pub protocol: String,
    /// 连接状态
    pub state: ConnectionState,
    /// 建立时间
    pub established_at: std::time::SystemTime,
    /// 关闭时间
    pub closed_at: Option<std::time::SystemTime>,
    /// 最后活动时间
    pub last_activity: std::time::SystemTime,
    /// 发送的数据包数量
    pub packets_sent: u64,
    /// 接收的数据包数量
    pub packets_received: u64,
    /// 发送的字节数
    pub bytes_sent: u64,
    /// 接收的字节数
    pub bytes_received: u64,
}

impl Default for ConnectionInfo {
    fn default() -> Self {
        let now = std::time::SystemTime::now();
        Self {
            session_id: SessionId::new(0),
            local_addr: "0.0.0.0:0".parse().unwrap(),
            peer_addr: "0.0.0.0:0".parse().unwrap(),
            protocol: "tcp".to_string(),
            state: ConnectionState::Connecting,
            established_at: now,
            closed_at: None,
            last_activity: now,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
}

impl ConnectionInfo {
    pub fn update_activity(&mut self) {
        self.last_activity = std::time::SystemTime::now();
    }
    
    pub fn record_packet_sent(&mut self, size: usize) {
        self.packets_sent += 1;
        self.bytes_sent += size as u64;
        self.update_activity();
    }
    
    pub fn record_packet_received(&mut self, size: usize) {
        self.packets_received += 1;
        self.bytes_received += size as u64;
        self.update_activity();
    }
    
    pub fn connection_duration(&self) -> std::time::Duration {
        self.established_at.elapsed().unwrap_or_default()
    }
    
    pub fn idle_duration(&self) -> std::time::Duration {
        self.last_activity.elapsed().unwrap_or_default()
    }
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// 正在连接
    Connecting,
    /// 已连接
    Connected,
    /// 正在关闭
    Closing,
    /// 已关闭
    Closed,
    /// 已暂停
    Paused,
    /// 错误状态
    Error,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionState::Connecting => write!(f, "Connecting"),
            ConnectionState::Connected => write!(f, "Connected"),
            ConnectionState::Closing => write!(f, "Closing"),
            ConnectionState::Closed => write!(f, "Closed"),
            ConnectionState::Paused => write!(f, "Paused"),
            ConnectionState::Error => write!(f, "Error"),
        }
    }
}

/// 协议特定命令trait
/// 
/// 此trait允许各协议定义自己的特定命令类型
pub trait ProtocolCommand: Send + std::fmt::Debug + 'static {
    type Response: Send;
    
    /// 转换为通用传输命令
    fn into_transport_command(self) -> TransportCommand;
    
    /// 执行命令
    fn execute(self) -> impl std::future::Future<Output = Self::Response> + Send;
}

/// 命令构建器
pub struct CommandBuilder;

impl CommandBuilder {
    /// 创建发送命令
    pub fn send(session_id: SessionId, packet: Packet) -> (TransportCommand, oneshot::Receiver<Result<(), TransportError>>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::Send {
            session_id,
            packet,
            response_tx,
        };
        (command, response_rx)
    }
    
    /// 创建关闭命令
    pub fn close(session_id: SessionId) -> (TransportCommand, oneshot::Receiver<Result<(), TransportError>>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::Close {
            session_id,
            response_tx,
        };
        (command, response_rx)
    }
    
    /// 创建获取统计信息命令
    pub fn get_stats() -> (TransportCommand, oneshot::Receiver<TransportStats>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::GetStats { response_tx };
        (command, response_rx)
    }
    
    /// 创建获取连接信息命令
    pub fn get_connection_info(session_id: SessionId) -> (TransportCommand, oneshot::Receiver<Result<ConnectionInfo, TransportError>>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::GetConnectionInfo {
            session_id,
            response_tx,
        };
        (command, response_rx)
    }
    
    /// 创建获取活跃会话命令
    pub fn get_active_sessions() -> (TransportCommand, oneshot::Receiver<Vec<SessionId>>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::GetActiveSessions { response_tx };
        (command, response_rx)
    }
    
    /// 创建强制断开命令
    pub fn force_disconnect(session_id: SessionId, reason: String) -> (TransportCommand, oneshot::Receiver<Result<(), TransportError>>) {
        let (response_tx, response_rx) = oneshot::channel();
        let command = TransportCommand::ForceDisconnect {
            session_id,
            reason,
            response_tx,
        };
        (command, response_rx)
    }
}

/// 命令执行器
pub struct CommandExecutor;

impl CommandExecutor {
    /// 执行发送命令并等待结果
    pub async fn send_and_wait(
        command_tx: &tokio::sync::mpsc::Sender<TransportCommand>,
        session_id: SessionId,
        packet: Packet,
    ) -> Result<(), TransportError> {
        let (command, response_rx) = CommandBuilder::send(session_id, packet);
        
        command_tx.send(command)
            .await
            .map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await
            .map_err(|_| TransportError::connection_error("Channel closed", false))?
    }
    
    /// 关闭连接并等待结果
    pub async fn close_and_wait(
        command_tx: &tokio::sync::mpsc::Sender<TransportCommand>,
        session_id: SessionId,
    ) -> Result<(), TransportError> {
        let (command, response_rx) = CommandBuilder::close(session_id);
        
        command_tx.send(command)
            .await
            .map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await
            .map_err(|_| TransportError::connection_error("Channel closed", false))?
    }
    
    /// 获取统计信息
    pub async fn get_stats(
        command_tx: &tokio::sync::mpsc::Sender<TransportCommand>,
    ) -> Result<TransportStats, TransportError> {
        let (command, response_rx) = CommandBuilder::get_stats();
        
        command_tx.send(command)
            .await
            .map_err(|_| TransportError::connection_error("Channel closed", false))?;
        
        response_rx.await
            .map_err(|_| TransportError::connection_error("Channel closed", false))
    }
} 