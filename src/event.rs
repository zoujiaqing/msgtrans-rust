use std::net::SocketAddr;
use crate::{SessionId, PacketId, CloseReason};
use crate::command::ConnectionInfo;
use crate::error::TransportError;
use crate::packet::Packet;

/// 请求上下文 - 封装一次性的请求响应能力
/// 
/// 🎯 设计目标：
/// - 简洁的响应接口：ctx.respond(response_packet)
/// - 类型安全：共享状态防止重复响应
/// - 自动填充：自动设置 response packet 的 message_id 和 packet_type
/// - 防御性设计：Drop 检测可以发现忘记响应的情况
/// - 支持克隆：允许在事件广播中使用
#[derive(Clone)]
pub struct RequestContext {
    /// 原始请求包
    pub request: Packet,
    /// 响应发送器（支持克隆，但只能使用一次）
    responder: std::sync::Arc<std::sync::Mutex<Option<Box<dyn FnOnce(Packet) + Send + 'static>>>>,
}

impl std::fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestContext")
            .field("request", &self.request)
            .field("responder", &"<FnOnce(Packet)>")
            .finish()
    }
}

impl RequestContext {
    /// 创建新的请求上下文
    pub fn new<F>(request: Packet, responder: F) -> Self 
    where 
        F: FnOnce(Packet) + Send + 'static,
    {
        Self {
            request,
            responder: std::sync::Arc::new(std::sync::Mutex::new(Some(Box::new(responder)))),
        }
    }
    
    /// 响应请求，自动填充响应头
    /// 
    /// 这个方法会：
    /// - 自动设置 response.packet_type = PacketType::Response
    /// - 自动设置 response.message_id = self.request.message_id
    /// - 发送响应到对端
    /// - 确保只能响应一次（通过互斥锁保护）
    pub fn respond(self, mut response: Packet) {
        use crate::packet::PacketType;
        
        // 自动填充响应头
        response.packet_type = PacketType::Response;
        response.message_id = self.request.message_id;
        response.set_packet_type(PacketType::Response);
        response.set_message_id(self.request.message_id);
        
        if let Ok(mut responder_guard) = self.responder.lock() {
            if let Some(responder) = responder_guard.take() {
                tracing::debug!("📤 响应请求: message_id={}", self.request.message_id);
                responder(response);
            } else {
                tracing::warn!(
                    "⚠️ 尝试重复响应请求: message_id={}",
                    self.request.message_id
                );
            }
        } else {
            tracing::error!(
                "❌ 无法获取响应器锁: message_id={}",
                self.request.message_id
            );
        }
    }
    
    /// 响应构造器：传入处理闭包
    /// 
    /// 便捷方法，允许使用闭包来构造响应：
    /// ```
    /// ctx.respond_with(|req| {
    ///     let result = handle_request(req);
    ///     Packet::response(req.message_id, result)
    /// });
    /// ```
    pub fn respond_with<F>(self, f: F) 
    where 
        F: FnOnce(&Packet) -> Packet,
    {
        let response = f(&self.request);
        self.respond(response);
    }
    
    /// 获取请求数据的只读引用
    pub fn request_data(&self) -> &[u8] {
        &self.request.payload
    }
    
    /// 获取请求ID
    pub fn request_id(&self) -> u32 {
        self.request.message_id
    }
}

impl Drop for RequestContext {
    fn drop(&mut self) {
        // 只有当这是最后一个引用且没有响应时才警告
        if std::sync::Arc::strong_count(&self.responder) == 1 {
            if let Ok(responder_guard) = self.responder.lock() {
                if responder_guard.is_some() {
                    tracing::warn!(
                        "⚠️ RequestContext dropped without responding to message_id={}. This indicates a missing ctx.respond() call in your business logic.",
                        self.request.message_id
                    );
                }
            }
        }
    }
}

/// 传输层事件的统一抽象
#[derive(Debug)]
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
    MessageReceived { 
        session_id: SessionId, 
        packet: Packet 
    },
    MessageSent { 
        session_id: SessionId, 
        packet_id: PacketId 
    },
    
    /// 🚀 新增：请求响应事件
    /// 
    /// 当收到 PacketType::Request 类型的包时触发此事件
    /// 服务端：包含 session_id，用于区分不同客户端的请求
    /// 客户端：通常不需要 session_id（只有一个连接）
    RequestReceived { 
        session_id: SessionId, 
        context: RequestContext 
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

impl Clone for TransportEvent {
    fn clone(&self) -> Self {
        match self {
            TransportEvent::ConnectionEstablished { session_id, info } => {
                TransportEvent::ConnectionEstablished { 
                    session_id: *session_id, 
                    info: info.clone() 
                }
            }
            TransportEvent::ConnectionClosed { session_id, reason } => {
                TransportEvent::ConnectionClosed { 
                    session_id: *session_id, 
                    reason: reason.clone() 
                }
            }
            TransportEvent::MessageReceived { session_id, packet } => {
                TransportEvent::MessageReceived { 
                    session_id: *session_id, 
                    packet: packet.clone() 
                }
            }
            TransportEvent::MessageSent { session_id, packet_id } => {
                TransportEvent::MessageSent { 
                    session_id: *session_id, 
                    packet_id: *packet_id 
                }
            }
            TransportEvent::RequestReceived { session_id, context } => {
                TransportEvent::RequestReceived {
                    session_id: *session_id,
                    context: context.clone()
                }
            }
            TransportEvent::TransportError { session_id, error } => {
                TransportEvent::TransportError { 
                    session_id: *session_id, 
                    error: error.clone() 
                }
            }
            TransportEvent::ServerStarted { address } => {
                TransportEvent::ServerStarted { address: *address }
            }
            TransportEvent::ServerStopped => TransportEvent::ServerStopped,
            TransportEvent::ClientConnected { address } => {
                TransportEvent::ClientConnected { address: *address }
            }
            TransportEvent::ClientDisconnected => TransportEvent::ClientDisconnected,
        }
    }
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
            TransportEvent::MessageReceived { session_id, .. } => Some(*session_id),
            TransportEvent::MessageSent { session_id, .. } => Some(*session_id),
            TransportEvent::RequestReceived { session_id, .. } => Some(*session_id),
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
            TransportEvent::MessageReceived { .. } | 
            TransportEvent::MessageSent { .. } |
            TransportEvent::RequestReceived { .. }
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
                    error: TransportError::connection_error(format!("IO error: {:?}", std::io::Error::new(std::io::ErrorKind::Other, error)), true)
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
                    error: TransportError::protocol_error("generic", error),
                }
            }
            _ => {
                // Ping/Pong 事件不需要转换为通用事件
                TransportEvent::TransportError {
                    session_id: self.session_id(),
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
    Connected { 
        info: crate::command::ConnectionInfo 
    },
    /// 连接关闭
    Disconnected { 
        reason: crate::error::CloseReason 
    },
    /// 收到消息
    MessageReceived { 
        packet: crate::packet::Packet 
    },
    /// 消息发送成功
    MessageSent { 
        packet_id: crate::PacketId 
    },
    /// 传输错误
    Error { 
        error: crate::error::TransportError 
    },
}

impl ClientEvent {
    /// 从TransportEvent转换为ClientEvent，隐藏会话ID
    pub fn from_transport_event(event: TransportEvent) -> Option<Self> {
        match event {
            TransportEvent::ConnectionEstablished { info, .. } => {
                Some(ClientEvent::Connected { info })
            }
            TransportEvent::ConnectionClosed { reason, .. } => {
                Some(ClientEvent::Disconnected { reason })
            }
            TransportEvent::MessageReceived { packet, .. } => {
                Some(ClientEvent::MessageReceived { packet })
            }
            TransportEvent::MessageSent { packet_id, .. } => {
                Some(ClientEvent::MessageSent { packet_id })
            }
            TransportEvent::RequestReceived { .. } => {
                // RequestReceived 不适用于客户端（客户端通常不处理请求）
                // 这个事件主要用于服务端
                None
            }
            TransportEvent::TransportError { error, .. } => {
                Some(ClientEvent::Error { error })
            }
            // 忽略服务器专用事件
            TransportEvent::ServerStarted { .. } | 
            TransportEvent::ServerStopped |
            TransportEvent::ClientConnected { .. } |
            TransportEvent::ClientDisconnected => None,
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