use std::net::SocketAddr;
use super::{SessionId, PacketId, CloseReason};
use super::command::ConnectionInfo;
use super::error::TransportError;
use super::packet::UnifiedPacket;

/// 传输层事件的统一抽象
#[derive(Clone, Debug)]
pub enum TransportEvent {
    /// 连接相关事件
    ConnectionEstablished { 
        session_id: SessionId, 
        info: ConnectionInfo 
    },
    ConnectionClosed { 
        session_id: SessionId, 
        reason: CloseReason 
    },
    
    /// 数据传输事件
    PacketReceived { 
        session_id: SessionId, 
        packet: UnifiedPacket 
    },
    PacketSent { 
        session_id: SessionId, 
        packet_id: PacketId 
    },
    
    /// 错误事件
    TransportError { 
        session_id: Option<SessionId>, 
        error: TransportError 
    },
    
    /// 服务器事件
    ServerStarted { 
        address: SocketAddr 
    },
    ServerStopped,
    
    /// 客户端事件
    ClientConnected { 
        address: SocketAddr 
    },
    ClientDisconnected,
}

/// 协议特定事件trait
/// 
/// 此trait允许各协议定义自己的特定事件类型，同时保持与统一事件系统的兼容性
pub trait ProtocolEvent: Clone + Send + std::fmt::Debug + 'static {
    /// 转换为通用传输事件
    fn into_transport_event(self) -> TransportEvent;
    
    /// 获取相关的会话ID（如果有）
    fn session_id(&self) -> Option<SessionId>;
    
    /// 判断是否为数据传输事件
    fn is_data_event(&self) -> bool;
    
    /// 判断是否为错误事件
    fn is_error_event(&self) -> bool;
}

/// 连接事件的简化表示
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    Established { 
        session_id: SessionId, 
        info: ConnectionInfo 
    },
    Closed { 
        session_id: SessionId, 
        reason: CloseReason 
    },
}

impl TransportEvent {
    /// 获取事件相关的会话ID
    pub fn session_id(&self) -> Option<SessionId> {
        match self {
            TransportEvent::ConnectionEstablished { session_id, .. } => Some(*session_id),
            TransportEvent::ConnectionClosed { session_id, .. } => Some(*session_id),
            TransportEvent::PacketReceived { session_id, .. } => Some(*session_id),
            TransportEvent::PacketSent { session_id, .. } => Some(*session_id),
            TransportEvent::TransportError { session_id, .. } => *session_id,
            _ => None,
        }
    }
    
    /// 判断是否为连接相关事件
    pub fn is_connection_event(&self) -> bool {
        matches!(self, 
            TransportEvent::ConnectionEstablished { .. } | 
            TransportEvent::ConnectionClosed { .. }
        )
    }
    
    /// 判断是否为数据传输事件
    pub fn is_data_event(&self) -> bool {
        matches!(self, 
            TransportEvent::PacketReceived { .. } | 
            TransportEvent::PacketSent { .. }
        )
    }
    
    /// 判断是否为错误事件
    pub fn is_error_event(&self) -> bool {
        matches!(self, TransportEvent::TransportError { .. })
    }
    
    /// 判断是否为服务器事件
    pub fn is_server_event(&self) -> bool {
        matches!(self, 
            TransportEvent::ServerStarted { .. } | 
            TransportEvent::ServerStopped
        )
    }
    
    /// 判断是否为客户端事件
    pub fn is_client_event(&self) -> bool {
        matches!(self, 
            TransportEvent::ClientConnected { .. } | 
            TransportEvent::ClientDisconnected
        )
    }
}

/// TCP协议特定事件
#[derive(Debug, Clone)]
pub enum TcpEvent {
    ListenerBound { 
        addr: SocketAddr 
    },
    AcceptError { 
        error: String 
    },
    ConnectionTimeout { 
        session_id: SessionId 
    },
}

impl ProtocolEvent for TcpEvent {
    fn into_transport_event(self) -> TransportEvent {
        match self {
            TcpEvent::ListenerBound { addr } => {
                TransportEvent::ServerStarted { address: addr }
            }
            TcpEvent::AcceptError { error } => {
                TransportEvent::TransportError {
                    session_id: None,
                    error: TransportError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)),
                }
            }
            TcpEvent::ConnectionTimeout { session_id } => {
                TransportEvent::ConnectionClosed {
                    session_id,
                    reason: CloseReason::Timeout,
                }
            }
        }
    }
    
    fn session_id(&self) -> Option<SessionId> {
        match self {
            TcpEvent::ConnectionTimeout { session_id } => Some(*session_id),
            _ => None,
        }
    }
    
    fn is_data_event(&self) -> bool {
        false
    }
    
    fn is_error_event(&self) -> bool {
        matches!(self, TcpEvent::AcceptError { .. })
    }
}

/// WebSocket协议特定事件
#[derive(Debug, Clone)]
pub enum WebSocketEvent {
    HandshakeCompleted { 
        session_id: SessionId 
    },
    PingReceived { 
        session_id: SessionId 
    },
    PongReceived { 
        session_id: SessionId 
    },
    InvalidFrame { 
        session_id: SessionId, 
        error: String 
    },
}

impl ProtocolEvent for WebSocketEvent {
    fn into_transport_event(self) -> TransportEvent {
        match self {
            WebSocketEvent::HandshakeCompleted { session_id } => {
                // 这个应该已经通过ConnectionEstablished处理了
                TransportEvent::ConnectionEstablished {
                    session_id,
                    info: ConnectionInfo::default(), // 临时实现
                }
            }
            WebSocketEvent::InvalidFrame { session_id, error } => {
                TransportEvent::TransportError {
                    session_id: Some(session_id),
                    error: TransportError::Protocol(error),
                }
            }
            _ => {
                // Ping/Pong 事件不需要转换为通用事件
                TransportEvent::TransportError {
                    session_id: self.session_id(),
                    error: TransportError::Protocol("Unhandled WebSocket event".to_string()),
                }
            }
        }
    }
    
    fn session_id(&self) -> Option<SessionId> {
        match self {
            WebSocketEvent::HandshakeCompleted { session_id } => Some(*session_id),
            WebSocketEvent::PingReceived { session_id } => Some(*session_id),
            WebSocketEvent::PongReceived { session_id } => Some(*session_id),
            WebSocketEvent::InvalidFrame { session_id, .. } => Some(*session_id),
        }
    }
    
    fn is_data_event(&self) -> bool {
        matches!(self, 
            WebSocketEvent::PingReceived { .. } | 
            WebSocketEvent::PongReceived { .. }
        )
    }
    
    fn is_error_event(&self) -> bool {
        matches!(self, WebSocketEvent::InvalidFrame { .. })
    }
}

/// QUIC协议特定事件
#[derive(Debug, Clone)]
pub enum QuicEvent {
    StreamOpened { 
        session_id: SessionId, 
        stream_id: u64 
    },
    StreamClosed { 
        session_id: SessionId, 
        stream_id: u64 
    },
    CertificateVerified { 
        session_id: SessionId 
    },
    ConnectionIdRetired { 
        session_id: SessionId, 
        connection_id: u64 
    },
}

impl ProtocolEvent for QuicEvent {
    fn into_transport_event(self) -> TransportEvent {
        match self {
            QuicEvent::StreamOpened { session_id, .. } => {
                // QUIC流开启不等同于连接建立，可能需要特殊处理
                TransportEvent::ConnectionEstablished {
                    session_id,
                    info: ConnectionInfo::default(),
                }
            }
            QuicEvent::StreamClosed { session_id, .. } => {
                TransportEvent::ConnectionClosed {
                    session_id,
                    reason: CloseReason::Normal,
                }
            }
            _ => {
                // 其他QUIC事件暂时不转换
                TransportEvent::TransportError {
                    session_id: self.session_id(),
                    error: TransportError::Protocol("Unhandled QUIC event".to_string()),
                }
            }
        }
    }
    
    fn session_id(&self) -> Option<SessionId> {
        match self {
            QuicEvent::StreamOpened { session_id, .. } => Some(*session_id),
            QuicEvent::StreamClosed { session_id, .. } => Some(*session_id),
            QuicEvent::CertificateVerified { session_id } => Some(*session_id),
            QuicEvent::ConnectionIdRetired { session_id, .. } => Some(*session_id),
        }
    }
    
    fn is_data_event(&self) -> bool {
        false
    }
    
    fn is_error_event(&self) -> bool {
        false
    }
} 