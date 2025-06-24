use std::net::SocketAddr;
use crate::{SessionId, PacketId, CloseReason};
use crate::command::ConnectionInfo;
use crate::error::TransportError;
use crate::packet::Packet;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::packet::PacketType;
use std::sync::{Arc, Mutex};

/// 传输层事件的统一抽象
#[derive(Debug, Clone)]
pub enum TransportEvent {
    /// 连接相关事件
    ConnectionEstablished { info: ConnectionInfo },
    ConnectionClosed { reason: CloseReason },
    
    /// 数据传输事件
    MessageReceived(Packet),
    MessageSent { packet_id: PacketId },
    
    /// 错误事件
    TransportError { error: TransportError },
    
    /// 服务器事件
    ServerStarted { address: SocketAddr },
    ServerStopped,
    
    /// 客户端事件
    ClientConnected { address: SocketAddr },
    ClientDisconnected,

    RequestReceived(Arc<RequestContext>),
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
            TransportEvent::ConnectionEstablished { .. } => None,
            TransportEvent::ConnectionClosed { .. } => None,
            TransportEvent::MessageReceived(..) => None,
            TransportEvent::MessageSent { .. } => None,
            TransportEvent::TransportError { .. } => None,
            TransportEvent::ServerStarted { .. } => None,
            TransportEvent::ServerStopped => None,
            TransportEvent::ClientConnected { .. } => None,
            TransportEvent::ClientDisconnected => None,
            TransportEvent::RequestReceived(..) => None,
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
            TransportEvent::MessageReceived(..) | 
            TransportEvent::RequestReceived(..)
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
                    error: TransportError::connection_error(format!("IO error: {:?}", std::io::Error::new(std::io::ErrorKind::Other, error)), true)
                }
            }
            TcpEvent::ConnectionTimeout { session_id } => {
                TransportEvent::ConnectionClosed {
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
                    info: ConnectionInfo::default(), // 临时实现
                }
            }
            WebSocketEvent::InvalidFrame { session_id, error } => {
                TransportEvent::TransportError {
                    error: TransportError::protocol_error("generic", error),
                }
            }
            _ => {
                // Ping/Pong 事件不需要转换为通用事件
                TransportEvent::TransportError {
                    error: TransportError::protocol_error("generic", "Unhandled WebSocket event".to_string()),
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
                    info: ConnectionInfo::default(),
                }
            }
            QuicEvent::StreamClosed { session_id, .. } => {
                TransportEvent::ConnectionClosed {
                    reason: CloseReason::Normal,
                }
            }
            _ => {
                // 其他QUIC事件暂时不转换
                TransportEvent::TransportError {
                    error: TransportError::protocol_error("generic", "Unhandled QUIC event".to_string()),
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

/// 客户端专用事件 - 隐藏会话ID概念
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// 连接建立
    Connected { info: crate::command::ConnectionInfo },
    /// 连接关闭
    Disconnected { reason: crate::error::CloseReason },
    /// 收到消息
    MessageReceived { packet: crate::packet::Packet },
    /// 消息发送成功
    MessageSent { packet_id: crate::PacketId },
    /// 传输错误
    Error { error: crate::error::TransportError },
    RequestReceived { ctx: Arc<RequestContext> },
}

impl ClientEvent {
    /// 从TransportEvent转换为ClientEvent，隐藏会话ID
    pub fn from_transport_event(event: TransportEvent) -> Option<Self> {
        match event {
            TransportEvent::ConnectionEstablished { info } =>
                Some(ClientEvent::Connected { info }),
            TransportEvent::ConnectionClosed { reason } =>
                Some(ClientEvent::Disconnected { reason }),
            TransportEvent::MessageReceived(packet) =>
                Some(ClientEvent::MessageReceived { packet }),
            TransportEvent::MessageSent { packet_id } =>
                Some(ClientEvent::MessageSent { packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ClientEvent::Error { error }),
            TransportEvent::RequestReceived(ctx) =>
                Some(ClientEvent::RequestReceived { ctx }),
            _ => None,
        }
    }
    
    /// 判断是否为连接相关事件
    pub fn is_connection_event(&self) -> bool {
        matches!(self, 
            ClientEvent::Connected { .. } | 
            ClientEvent::Disconnected { .. }
        )
    }
    
    /// 判断是否为数据传输事件
    pub fn is_data_event(&self) -> bool {
        matches!(self, 
            ClientEvent::MessageReceived { .. } | 
            ClientEvent::MessageSent { .. }
        )
    }
    
    /// 判断是否为错误事件
    pub fn is_error_event(&self) -> bool {
        matches!(self, ClientEvent::Error { .. })
    }
}

/// 服务端专用事件 - 必须带 session_id
#[derive(Debug, Clone)]
pub enum ServerEvent {
    ConnectionEstablished { session_id: SessionId, info: ConnectionInfo },
    ConnectionClosed { session_id: SessionId, reason: CloseReason },
    MessageReceived { session_id: SessionId, packet: Packet },
    MessageSent { session_id: SessionId, packet_id: PacketId },
    TransportError { session_id: Option<SessionId>, error: TransportError },
    ServerStarted { address: SocketAddr },
    ServerStopped,
    RequestReceived { session_id: SessionId, ctx: Arc<RequestContext> },
}

impl ServerEvent {
    pub fn from_transport_event(event: TransportEvent) -> Option<Self> {
        match event {
            TransportEvent::ConnectionEstablished { info } =>
                None,
            TransportEvent::ConnectionClosed { reason } =>
                None,
            TransportEvent::MessageReceived(packet) =>
                None,
            TransportEvent::MessageSent { packet_id } =>
                None,
            TransportEvent::TransportError { error } =>
                None,
            TransportEvent::ServerStarted { address } =>
                Some(ServerEvent::ServerStarted { address }),
            TransportEvent::ServerStopped =>
                Some(ServerEvent::ServerStopped),
            TransportEvent::RequestReceived(ctx) =>
                None,
            _ => None,
        }
    }

    /// 新增：通过 session_id 将 TransportEvent 转为 ServerEvent
    pub fn from_transport_event_with_session(event: TransportEvent, session_id: SessionId) -> Option<Self> {
        match event {
            TransportEvent::ConnectionEstablished { info } =>
                Some(ServerEvent::ConnectionEstablished { session_id, info }),
            TransportEvent::ConnectionClosed { reason } =>
                Some(ServerEvent::ConnectionClosed { session_id, reason }),
            TransportEvent::MessageReceived(packet) =>
                Some(ServerEvent::MessageReceived { session_id, packet }),
            TransportEvent::MessageSent { packet_id } =>
                Some(ServerEvent::MessageSent { session_id, packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ServerEvent::TransportError { session_id: Some(session_id), error }),
            TransportEvent::RequestReceived(ctx) =>
                Some(ServerEvent::RequestReceived { session_id, ctx }),
            TransportEvent::ServerStarted { address } =>
                Some(ServerEvent::ServerStarted { address }),
            TransportEvent::ServerStopped =>
                Some(ServerEvent::ServerStopped),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct RequestContext {
    pub session_id: SessionId,
    pub request: Packet,
    responder: Arc<Mutex<Option<Box<dyn FnOnce(Packet) + Send + 'static>>>>,
    responded: Arc<AtomicBool>,
}

impl RequestContext {
    pub fn new(
        session_id: SessionId,
        request: Packet,
        responder: Box<dyn FnOnce(Packet) + Send + 'static>,
    ) -> Self {
        Self {
            session_id,
            request,
            responder: Arc::new(Mutex::new(Some(responder))),
            responded: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn respond(self: Arc<Self>, mut response: Packet) {
        if self.responded.swap(true, Ordering::SeqCst) {
            log::warn!("Request already responded");
            return;
        }
        tracing::debug!("🔄 RequestContext::respond called: 原类型={:?}, ID={}", response.packet_type, response.message_id);
        
        // 🔧 修复：同时设置两个包类型字段
        response.packet_type = PacketType::Response;
        response.header.packet_type = PacketType::Response;
        response.message_id = self.request.message_id;
        response.header.message_id = self.request.message_id;
        
        tracing::debug!("🔄 RequestContext::respond 设置后: 新类型={:?}, header类型={:?}, ID={}", 
            response.packet_type, response.header.packet_type, response.message_id);
        
        if let Some(responder) = self.responder.lock().unwrap().take() {
            tracing::debug!("🔄 RequestContext::respond 调用发送回调，包类型={:?}, header类型={:?}, ID={}", 
                response.packet_type, response.header.packet_type, response.message_id);
            responder(response);
        } else {
            tracing::warn!("⚠️ RequestContext::respond 没有发送回调可用");
        }
    }

    pub fn respond_with<F: FnOnce(&Packet) -> Packet>(self: Arc<Self>, f: F) {
        let response = f(&self.request);
        self.respond(response);
    }
}

impl Drop for RequestContext {
    fn drop(&mut self) {
        if !self.responded.load(Ordering::SeqCst) {
            log::warn!("RequestContext dropped without response (可能漏响应)");
        }
    }
}

impl std::fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestContext")
            .field("session_id", &self.session_id)
            .field("request", &self.request)
            .finish()
    }
} 