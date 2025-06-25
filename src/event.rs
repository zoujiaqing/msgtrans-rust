use std::net::SocketAddr;
use crate::{SessionId, PacketId, CloseReason};
use crate::command::ConnectionInfo;
use crate::error::TransportError;
use crate::packet::Packet;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::packet::PacketType;
use std::sync::{Arc, Mutex};
use std::time::Instant;

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
            responder: self.responder.clone(), // 🔧 修复：共享响应状态
            responded: self.responded.clone(), // 🔧 克隆实例不是主实例，不负责检查响应
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
    
    /// 🎯 收到消息（统一上下文，包含所有信息）
    MessageReceived(TransportContext),
    
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
    
    /// 🎯 收到消息（统一上下文，包含所有信息）
    MessageReceived { session_id: SessionId, context: TransportContext },
    
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
                         TransportEvent::MessageReceived(packet) => {
                 // 转换为统一的TransportContext
                 let context = TransportContext::new_oneway(
                     Some(session_id), 
                     packet.header.message_id, 
                     packet.payload.clone()
                 );
                 Some(ServerEvent::MessageReceived { session_id, context })
             }
             TransportEvent::MessageSent { packet_id } =>
                 Some(ServerEvent::MessageSent { session_id, message_id: packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ServerEvent::TransportError { session_id: Some(session_id), error }),
                         TransportEvent::RequestReceived(ctx) => {
                 // 🔧 将RequestContext转换为TransportContext
                 let transport_ctx = TransportContext::new_request(
                     Some(session_id),
                     ctx.request_id,
                     ctx.data.clone(),
                     ctx.responder.clone()
                 );
                 Some(ServerEvent::MessageReceived { session_id, context: transport_ctx })
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
                        // Request包由TransportClient特殊处理
                        None
                    }
                    _ => {
                        // OneWay和Response包正常处理
                        let context = TransportContext::new_oneway(
                            None, 
                            packet.header.message_id, 
                            packet.payload.clone()
                        );
                        Some(ClientEvent::MessageReceived(context))
                    }
                }
            }
                         TransportEvent::MessageSent { packet_id } =>
                 Some(ClientEvent::MessageSent { message_id: packet_id }),
            TransportEvent::TransportError { error } =>
                Some(ClientEvent::Error { error }),

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
            ClientEvent::MessageReceived(..) | 
            ClientEvent::MessageSent { .. }
        )
    }
    
    /// 判断是否为错误事件
    pub fn is_error_event(&self) -> bool {
        matches!(self, ClientEvent::Error { .. })
    }
}

/// 🎯 统一的传输上下文 - 用于所有接收的消息
#[derive(Clone)]
pub struct TransportContext {
    /// 消息来源会话ID（客户端为None，服务端为Some）
    pub peer: Option<SessionId>,
    /// 系统分配的消息ID
    pub message_id: u32,
    /// 解压后的纯数据
    pub data: Vec<u8>,
    /// 接收时间戳
    pub timestamp: Instant,
    /// 消息类型（内部使用）
    kind: TransportContextKind,
}

/// 消息类型枚举
#[derive(Clone)]
enum TransportContextKind {
    /// 单向消息（不需要响应）
    OneWay,
    /// 请求消息（需要响应）
    Request {
        responder: Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>,
        responded: Arc<AtomicBool>,
        is_primary: bool, // 标记是否为主实例
    },
}

impl TransportContext {
    /// 创建单向消息上下文
    pub fn new_oneway(
        peer: Option<SessionId>,
        message_id: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            peer,
            message_id,
            data,
            timestamp: Instant::now(),
            kind: TransportContextKind::OneWay,
        }
    }

    /// 创建请求消息上下文
    pub fn new_request(
        peer: Option<SessionId>,
        message_id: u32,
        data: Vec<u8>,
        responder: Arc<dyn Fn(Vec<u8>) + Send + Sync + 'static>,
    ) -> Self {
        Self {
            peer,
            message_id,
            data,
            timestamp: Instant::now(),
            kind: TransportContextKind::Request {
                responder,
                responded: Arc::new(AtomicBool::new(false)),
                is_primary: false, // 默认不是主实例
            },
        }
    }

    /// 设置为主实例（负责检查响应状态）
    pub(crate) fn set_primary(&mut self) {
        if let TransportContextKind::Request { is_primary, .. } = &mut self.kind {
            *is_primary = true;
        }
    }

    /// 检查是否为请求类型
    pub fn is_request(&self) -> bool {
        matches!(self.kind, TransportContextKind::Request { .. })
    }
    
    /// 将数据转换为字符串（损失转换）
    pub fn as_text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.data).to_string()
    }

    /// 响应请求（仅请求类型可用）
    pub fn respond(mut self, response: Vec<u8>) {
        match &mut self.kind {
            TransportContextKind::Request { responder, responded, .. } => {
                if responded.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                    responder(response);
                } else {
                    tracing::warn!("⚠️ TransportContext 已经响应过了 (ID: {})", self.message_id);
                }
            }
            TransportContextKind::OneWay => {
                tracing::warn!("⚠️ 无法响应单向消息 (ID: {})", self.message_id);
            }
        }
    }

    /// 便利方法：响应字节数据
    pub fn respond_bytes(self, response: &[u8]) {
        self.respond(response.to_vec());
    }
}

impl std::fmt::Debug for TransportContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransportContext")
            .field("peer", &self.peer)
            .field("message_id", &self.message_id)
            .field("data", &format!("{} bytes", self.data.len()))
            .field("timestamp", &self.timestamp)
            .field("is_request", &self.is_request())
            .finish()
    }
}

impl Drop for TransportContext {
    fn drop(&mut self) {
        if let TransportContextKind::Request { responded, is_primary, .. } = &self.kind {
            // 只有主实例才检查响应状态
            if *is_primary && !responded.load(Ordering::SeqCst) {
                tracing::warn!("⚠️ TransportContext被丢弃但未响应 (ID: {})", self.message_id);
            }
        }
    }
}

/// 🎯 统一的传输结果 - 用于所有发送操作的返回值
#[derive(Debug, Clone)]
pub struct TransportResult {
    /// 目标会话ID（客户端为None，服务端为Some）
    pub peer: Option<SessionId>,
    /// 系统分配的消息ID
    pub message_id: u32,
    /// 发送时间戳
    pub timestamp: Instant,
    /// 响应数据（仅request有，send为None）
    pub data: Option<Vec<u8>>,
    /// 传输状态
    pub status: TransportStatus,
}

/// 传输状态枚举
#[derive(Debug, Clone, PartialEq)]
pub enum TransportStatus {
    /// 发送成功
    Sent,
    /// 请求超时
    Timeout,
    /// 连接错误
    ConnectionError,
    /// 发送成功并收到响应
    Completed,
}

impl TransportResult {
    /// 创建发送结果
    pub fn new_sent(peer: Option<SessionId>, message_id: u32) -> Self {
        Self {
            peer,
            message_id,
            timestamp: Instant::now(),
            data: None,
            status: TransportStatus::Sent,
        }
    }

    /// 创建请求完成结果
    pub fn new_completed(peer: Option<SessionId>, message_id: u32, data: Vec<u8>) -> Self {
        Self {
            peer,
            message_id,
            timestamp: Instant::now(),
            data: Some(data),
            status: TransportStatus::Completed,
        }
    }

    /// 创建超时结果
    pub fn new_timeout(peer: Option<SessionId>, message_id: u32) -> Self {
        Self {
            peer,
            message_id,
            timestamp: Instant::now(),
            data: None,
            status: TransportStatus::Timeout,
        }
    }
    
    /// 创建连接错误结果
    pub fn new_connection_error(peer: Option<SessionId>, message_id: u32) -> Self {
        Self {
            peer,
            message_id,
            timestamp: Instant::now(),
            data: None,
            status: TransportStatus::ConnectionError,
        }
    }

    /// 检查是否发送成功
    pub fn is_sent(&self) -> bool {
        matches!(self.status, TransportStatus::Sent | TransportStatus::Completed)
    }

    /// 检查是否有响应数据
    pub fn has_response(&self) -> bool {
        self.data.is_some()
    }
} 