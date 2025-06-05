use std::sync::Arc;
use futures::{Sink, Stream};
use async_trait::async_trait;
use crate::packet::Packet;

pub mod quic;
pub mod quic_simple;
pub mod tcp;
pub mod websocket;
pub use websocket::WebSocketEvent;

/// 传输错误类型
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Channel full")]
    ChannelFull,
    #[error("Send error: {0}")]
    SendError(String),
    #[error("Receive error: {0}")]
    ReceiveError(String),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Connection error: {0}")]
    ConnectionError(String),
    #[error("Not connected")]
    NotConnected,
}

/// 传输事件
#[derive(Debug, Clone)]
pub enum TransportEvent {
    PacketReceived(Packet),
    Connected { remote_addr: std::net::SocketAddr },
    Disconnected { reason: String },
    Error(String),
}

/// 会话信息
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: usize,
    pub protocol: String,
    pub peer_address: String,
}

/// 传输控制接口
pub trait TransportControl: Send + Sync + Clone {
    fn session_info(&self) -> &SessionInfo;
    fn session_id(&self) -> usize {
        self.session_info().session_id
    }
    async fn close(&self) -> Result<(), TransportError>;
}

/// 协议特化 trait - 每个协议都需要实现
pub trait ProtocolSpecific: Send + Sync + 'static {
    type Sender: Sink<Packet, Error = TransportError> + Send + Clone + Unpin;
    type Receiver: Stream<Item = Result<Packet, TransportError>> + Send + Clone + Unpin;
    type Control: TransportControl;

    fn create_sender(&self) -> Self::Sender;
    fn create_receiver(&self) -> Self::Receiver;
    fn create_control(&self) -> Self::Control;
}

/// 统一的传输层接口
pub struct Transport<T: ProtocolSpecific> {
    inner: Arc<T>,
    sender: T::Sender,
    receiver: T::Receiver,
    control: T::Control,
}

impl<T: ProtocolSpecific> Transport<T> {
    pub fn new(protocol: Arc<T>) -> Self {
        let sender = protocol.create_sender();
        let receiver = protocol.create_receiver();
        let control = protocol.create_control();
        
        Self {
            inner: protocol,
            sender,
            receiver,
            control,
        }
    }



    /// 获取发送器
    pub fn sender(&self) -> T::Sender {
        self.sender.clone()
    }

    /// 获取接收器
    pub fn receiver(&self) -> T::Receiver {
        self.receiver.clone()
    }

    /// 获取控制接口
    pub fn control(&self) -> T::Control {
        self.control.clone()
    }

    /// 分离成独立组件
    pub fn split(self) -> (T::Sender, T::Receiver, T::Control) {
        (self.sender, self.receiver, self.control)
    }
}

/// 转换为传输层的 trait
#[async_trait]
pub trait IntoTransport<T: ProtocolSpecific> {
    async fn into_transport(self) -> Result<Transport<T>, TransportError>;
}

// 重新导出核心类型
pub use quic::{QuicConnection, QuicSender, QuicReceiver, QuicControl};
pub use tcp::{TcpConnection, TcpSender, TcpReceiver, TcpControl};
pub use websocket::{WebSocketConnection, WebSocketSender, WebSocketReceiver, WebSocketControl}; 