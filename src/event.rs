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

    RequestReceived(RequestContext),
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

/// 🎯 用户友好的消息结构 - 已解包的纯数据
#[derive(Debug, Clone)]
pub struct Message {
    /// 消息来源会话（客户端为None，服务端为Some）
    pub peer: Option<SessionId>,
    /// 已解压和解包的原始数据
    pub data: Vec<u8>,
    /// 消息ID（用于调试和日志）
    pub message_id: u32,
}

impl Message {
    /// 尝试将消息数据转换为UTF-8字符串
    pub fn as_text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.data.clone())
    }
    
    /// 将消息数据转换为UTF-8字符串（lossy）
    pub fn as_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
    
    /// 获取原始字节数据
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
}

/// 🎯 用户友好的请求上下文 - 已解包，提供简单响应接口
pub struct RequestContext {
    /// 请求来源会话（客户端为None，服务端为Some）
    pub peer: Option<SessionId>,
    /// 已解压和解包的请求数据
    pub data: Vec<u8>,
    /// 请求ID（用于调试和日志）
    pub request_id: u32,
    /// 响应回调（内部处理所有协议细节）
    responder: Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>,
    /// 确保只响应一次（使用Arc共享状态）
    responded: Arc<std::sync::atomic::AtomicBool>,
    /// 🔧 标记：是否为负责检查响应的主实例（防止clone实例触发警告）
    is_primary: bool,
}

impl RequestContext {
    /// 创建新的请求上下文
    pub fn new(
        peer: Option<SessionId>,
        data: Vec<u8>,
        request_id: u32,
        responder: Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>,
    ) -> Self {
        Self {
            peer,
            data,
            request_id,
            responder,
            responded: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            is_primary: false, // 🔧 新创建的实例默认不是主实例，等待事件转发时设置
        }
    }
    
    /// 尝试将请求数据转换为UTF-8字符串
    pub fn as_text(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.data.clone())
    }
    
    /// 将请求数据转换为UTF-8字符串（lossy）
    pub fn as_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }
    
    /// 获取原始字节数据
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }
    
    /// 🎯 使用字符串响应请求
    pub fn respond_text(&mut self, response: &str) {
        self.respond_bytes(response.as_bytes());
    }
    
    /// 🎯 使用字节数据响应请求
    pub fn respond_bytes(&mut self, response: &[u8]) {
        if self.responded.compare_exchange(false, true, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst).is_ok() {
            (self.responder)(response.to_vec());
        } else {
            tracing::warn!("⚠️ RequestContext 已经响应过了 (ID: {})", self.request_id);
        }
    }
    
    /// 🔧 设置为主实例（负责检查响应状态）
    pub(crate) fn set_primary(&mut self) {
        self.is_primary = true;
    }
}

impl Clone for RequestContext {
    fn clone(&self) -> Self {
        Self {
            peer: self.peer,
            data: self.data.clone(),
            request_id: self.request_id,
            responder: self.responder.clone(),
            responded: self.responded.clone(), // 🔧 修复：共享响应状态
            is_primary: false, // 🔧 克隆实例不是主实例，不负责检查响应
        }
    }
}

impl std::fmt::Debug for RequestContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RequestContext")
            .field("peer", &self.peer)
            .field("data", &format!("{} bytes", self.data.len()))
            .field("request_id", &self.request_id)
            .field("responded", &self.responded.load(std::sync::atomic::Ordering::SeqCst))
            .finish()
    }
}

impl Drop for RequestContext {
    fn drop(&mut self) {
        // 🔧 只有主实例才负责检查响应状态，避免clone实例触发误报警告
        if self.is_primary && !self.responded.load(std::sync::atomic::Ordering::SeqCst) {
            tracing::warn!("⚠️ RequestContext被丢弃但未响应 (ID: {})", self.request_id);
        }
    }
}

/// 🎯 用户友好的客户端事件 - 完全隐藏Packet复杂性
#[derive(Debug, Clone)]
pub enum ClientEvent {
    /// 连接已建立
    Connected { info: ConnectionInfo },
    /// 连接已断开
    Disconnected { reason: CloseReason },
    
    /// 🎯 收到消息（已解包）
    MessageReceived(Message),
    
    /// 🎯 收到请求（已解包，可直接响应）
    RequestReceived(RequestContext),
    
    /// 消息发送确认
    MessageSent { message_id: u32 },
    
    /// 传输错误
    Error { error: TransportError },
}

/// 🎯 用户友好的服务端事件 - 完全隐藏Packet复杂性  
#[derive(Debug, Clone)]
pub enum ServerEvent {
    /// 新连接建立
    ConnectionEstablished { session_id: SessionId, info: ConnectionInfo },
    /// 连接关闭
    ConnectionClosed { session_id: SessionId, reason: CloseReason },
    
    /// 🎯 收到消息（已解包）
    MessageReceived { session_id: SessionId, message: Message },
    
    /// 🎯 收到请求（已解包，可直接响应）
    RequestReceived { session_id: SessionId, request: RequestContext },
    
    /// 消息发送确认
    MessageSent { session_id: SessionId, message_id: u32 },
    
    /// 传输错误
    TransportError { session_id: Option<SessionId>, error: TransportError },
    
    /// 服务器生命周期事件
    ServerStarted { address: std::net::SocketAddr },
    ServerStopped,
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
                 Some(ServerEvent::MessageReceived { session_id, message: Message { peer: Some(session_id), data: packet.payload.clone(), message_id: packet.header.message_id } }),
             TransportEvent::MessageSent { packet_id } =>
                 Some(ServerEvent::MessageSent { session_id, message_id: packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ServerEvent::TransportError { session_id: Some(session_id), error }),
                         TransportEvent::RequestReceived(ctx) => {
                 // 🔧 克隆实例用于用户处理，主实例管理在Transport层
                 Some(ServerEvent::RequestReceived { session_id, request: ctx })
             }
            TransportEvent::ServerStarted { address } =>
                Some(ServerEvent::ServerStarted { address }),
            TransportEvent::ServerStopped =>
                Some(ServerEvent::ServerStopped),
            _ => None,
        }
    }
}

impl ClientEvent {
    /// 从TransportEvent转换为ClientEvent，隐藏会话ID
    pub fn from_transport_event(event: TransportEvent) -> Option<Self> {
        match event {
            TransportEvent::ConnectionEstablished { info } =>
                Some(ClientEvent::Connected { info }),
            TransportEvent::ConnectionClosed { reason } =>
                Some(ClientEvent::Disconnected { reason }),
            TransportEvent::MessageReceived(packet) => {
                match packet.header.packet_type {
                    crate::packet::PacketType::Request => {
                        // Request包由Transport处理并发送RequestReceived事件
                        None
                    }
                    _ => {
                        // OneWay和Response包正常处理
                        Some(ClientEvent::MessageReceived(Message { peer: None, data: packet.payload.clone(), message_id: packet.header.message_id }))
                    }
                }
            }
                         TransportEvent::MessageSent { packet_id } =>
                 Some(ClientEvent::MessageSent { message_id: packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ClientEvent::Error { error }),
                         TransportEvent::RequestReceived(ctx) => {
                 // 🔧 克隆实例用于用户处理，主实例管理在Transport层
                 Some(ClientEvent::RequestReceived(ctx))
             }
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