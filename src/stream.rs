use std::pin::Pin;
use std::task::{Context, Poll};
use futures::{Stream, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use crate::{
    SessionId,
    event::TransportEvent,
    packet::Packet,
};

/// 统一事件流
/// 
/// 提供了对传输事件的流式访问，支持过滤和转换
pub struct EventStream {
    /// 内部广播流
    inner: BroadcastStream<TransportEvent>,
    /// 可选的会话过滤器
    session_filter: Option<SessionId>,
}

impl EventStream {
    /// 创建新的事件流
    pub fn new(receiver: broadcast::Receiver<TransportEvent>) -> Self {
        Self {
            inner: BroadcastStream::new(receiver),
            session_filter: None,
        }
    }
    
    /// 创建带会话过滤的事件流
    pub fn with_session_filter(receiver: broadcast::Receiver<TransportEvent>, session_id: SessionId) -> Self {
        Self {
            inner: BroadcastStream::new(receiver),
            session_filter: Some(session_id),
        }
    }
    
    /// 过滤特定会话的事件
    pub fn filter_session(mut self, session_id: SessionId) -> Self {
        self.session_filter = Some(session_id);
        self
    }
    
    /// 获取下一个连接事件
    pub async fn next_connection_event(&mut self) -> Option<TransportEvent> {
        while let Some(result) = self.inner.next().await {
            if let Ok(event) = result {
                if self.should_emit_event(&event) && event.is_connection_event() {
                    return Some(event);
                }
            }
        }
        None
    }
    
    /// 获取下一个数据事件
    pub async fn next_data_event(&mut self) -> Option<TransportEvent> {
        while let Some(result) = self.inner.next().await {
            if let Ok(event) = result {
                if self.should_emit_event(&event) && event.is_data_event() {
                    return Some(event);
                }
            }
        }
        None
    }
    
    /// 获取下一个错误事件
    pub async fn next_error_event(&mut self) -> Option<TransportEvent> {
        while let Some(result) = self.inner.next().await {
            if let Ok(event) = result {
                if self.should_emit_event(&event) && event.is_error_event() {
                    return Some(event);
                }
            }
        }
        None
    }
    
    /// 检查是否应该发出事件（基于过滤器）
    fn should_emit_event(&self, event: &TransportEvent) -> bool {
        match self.session_filter {
            Some(filter_session_id) => {
                match event.session_id() {
                    Some(event_session_id) => event_session_id == filter_session_id,
                    None => true, // 全局事件总是发出
                }
            }
            None => true, // 没有过滤器，发出所有事件
        }
    }
}

impl Stream for EventStream {
    type Item = TransportEvent;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(event))) => {
                    if self.should_emit_event(&event) {
                        return Poll::Ready(Some(event));
                    }
                    // 继续循环，获取下一个事件
                }
                Poll::Ready(Some(Err(_))) => {
                    // 忽略接收错误，继续尝试
                    continue;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

/// 泛型接收器
/// 
/// 解决了原始Stream trait实现中Future状态管理的问题
/// 通过保持Future状态避免了poll_next时重新创建Future的问题
pub struct GenericReceiver<T> {
    /// 内部接收器
    receiver: mpsc::Receiver<T>,
    /// 当前的接收Future（可能为None）
    #[allow(dead_code)]
    recv_future: Option<Pin<Box<dyn std::future::Future<Output = Option<T>> + Send>>>,
}

impl<T: Send + 'static> GenericReceiver<T> {
    /// 创建新的泛型接收器
    pub fn new(receiver: mpsc::Receiver<T>) -> Self {
        Self {
            receiver,
            recv_future: None,
        }
    }
    
    /// 非阻塞尝试接收
    pub fn try_recv(&mut self) -> Result<T, mpsc::error::TryRecvError> {
        self.receiver.try_recv()
    }
    
    /// 异步接收
    pub async fn recv(&mut self) -> Option<T> {
        self.receiver.recv().await
    }
    
    /// 关闭接收器
    pub fn close(&mut self) {
        self.receiver.close();
    }
}

impl<T: Send + 'static> Stream for GenericReceiver<T> {
    type Item = T;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // 直接轮询接收器
        self.receiver.poll_recv(cx)
    }
}

/// 数据包流
/// 
/// 专门用于处理数据包的流
pub struct PacketStream {
    /// 内部事件流
    event_stream: EventStream,
}

impl PacketStream {
    /// 创建新的数据包流
    pub fn new(receiver: broadcast::Receiver<TransportEvent>) -> Self {
        Self {
            event_stream: EventStream::new(receiver),
        }
    }
    
    /// 创建带会话过滤的数据包流
    pub fn with_session_filter(receiver: broadcast::Receiver<TransportEvent>, session_id: SessionId) -> Self {
        Self {
            event_stream: EventStream::with_session_filter(receiver, session_id),
        }
    }
    
    /// 获取下一个数据包
    pub async fn next_packet(&mut self) -> Option<(SessionId, Packet)> {
        while let Some(event) = self.event_stream.next().await {
            if let TransportEvent::MessageReceived { session_id, packet } = event {
                return Some((session_id, packet));
            }
        }
        None
    }
}

impl Stream for PacketStream {
    type Item = (SessionId, Packet);
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.event_stream).poll_next(cx) {
                Poll::Ready(Some(TransportEvent::MessageReceived { session_id, packet })) => {
                    return Poll::Ready(Some((session_id, packet)));
                }
                Poll::Ready(Some(_)) => {
                    // 其他事件，继续循环
                    continue;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

/// 连接事件流
/// 
/// 专门处理连接建立和关闭事件
pub struct ConnectionStream {
    /// 内部事件流
    event_stream: EventStream,
}

impl ConnectionStream {
    /// 创建新的连接事件流
    pub fn new(receiver: broadcast::Receiver<TransportEvent>) -> Self {
        Self {
            event_stream: EventStream::new(receiver),
        }
    }
    
    /// 获取下一个连接事件
    pub async fn next_connection(&mut self) -> Option<ConnectionEvent> {
        while let Some(event) = self.event_stream.next().await {
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    return Some(ConnectionEvent::Established { session_id, info });
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    return Some(ConnectionEvent::Closed { session_id, reason });
                }
                _ => continue,
            }
        }
        None
    }
}

impl Stream for ConnectionStream {
    type Item = ConnectionEvent;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match Pin::new(&mut self.event_stream).poll_next(cx) {
                Poll::Ready(Some(TransportEvent::ConnectionEstablished { session_id, info })) => {
                    return Poll::Ready(Some(ConnectionEvent::Established { session_id, info }));
                }
                Poll::Ready(Some(TransportEvent::ConnectionClosed { session_id, reason })) => {
                    return Poll::Ready(Some(ConnectionEvent::Closed { session_id, reason }));
                }
                Poll::Ready(Some(_)) => {
                    // 其他事件，继续循环
                    continue;
                }
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

/// 连接事件
#[derive(Debug, Clone)]
pub enum ConnectionEvent {
    /// 连接建立
    Established { 
        session_id: SessionId, 
        info: crate::command::ConnectionInfo 
    },
    /// 连接关闭
    Closed { 
        session_id: SessionId, 
        reason: crate::CloseReason 
    },
}

/// 流工厂
/// 
/// 提供创建各种类型流的便捷方法
pub struct StreamFactory;

impl StreamFactory {
    /// 创建事件流
    pub fn event_stream(receiver: broadcast::Receiver<TransportEvent>) -> EventStream {
        EventStream::new(receiver)
    }
    
    /// 创建数据包流
    pub fn packet_stream(receiver: broadcast::Receiver<TransportEvent>) -> PacketStream {
        PacketStream::new(receiver)
    }
    
    /// 创建连接流
    pub fn connection_stream(receiver: broadcast::Receiver<TransportEvent>) -> ConnectionStream {
        ConnectionStream::new(receiver)
    }
    
    /// 创建会话过滤的事件流
    pub fn session_event_stream(
        receiver: broadcast::Receiver<TransportEvent>, 
        session_id: SessionId
    ) -> EventStream {
        EventStream::with_session_filter(receiver, session_id)
    }
    
    /// 创建会话过滤的数据包流
    pub fn session_packet_stream(
        receiver: broadcast::Receiver<TransportEvent>, 
        session_id: SessionId
    ) -> PacketStream {
        PacketStream::with_session_filter(receiver, session_id)
    }
    
    /// 创建客户端事件流（隐藏会话ID）
    pub fn client_event_stream(receiver: tokio::sync::broadcast::Receiver<crate::event::TransportEvent>) -> ClientEventStream {
        ClientEventStream::new(receiver)
    }
}

/// 流组合器
/// 
/// 提供流的组合和转换功能
pub struct StreamCombinator;

impl StreamCombinator {
    /// 合并多个事件流
    pub fn merge_event_streams(
        streams: Vec<EventStream>
    ) -> impl Stream<Item = TransportEvent> {
        futures::stream::select_all(streams)
    }
    
    /// 将事件流转换为数据包流
    pub fn events_to_packets(
        event_stream: EventStream
    ) -> impl Stream<Item = (SessionId, Packet)> {
        event_stream.filter_map(|event| async move {
            match event {
                TransportEvent::MessageReceived { session_id, packet } => {
                    Some((session_id, packet))
                }
                _ => None,
            }
        })
    }
    
    /// 将事件流转换为连接事件流
    pub fn events_to_connections(
        event_stream: EventStream
    ) -> impl Stream<Item = ConnectionEvent> {
        event_stream.filter_map(|event| async move {
            match event {
                TransportEvent::ConnectionEstablished { session_id, info } => {
                    Some(ConnectionEvent::Established { session_id, info })
                }
                TransportEvent::ConnectionClosed { session_id, reason } => {
                    Some(ConnectionEvent::Closed { session_id, reason })
                }
                _ => None,
            }
        })
    }
}

// 为broadcast::Receiver添加扩展trait
pub trait ReceiverExt {
    /// 转换为事件流
    fn into_event_stream(self) -> EventStream;
    
    /// 转换为数据包流
    fn into_packet_stream(self) -> PacketStream;
    
    /// 转换为连接流
    fn into_connection_stream(self) -> ConnectionStream;
}

impl ReceiverExt for broadcast::Receiver<TransportEvent> {
    fn into_event_stream(self) -> EventStream {
        EventStream::new(self)
    }
    
    fn into_packet_stream(self) -> PacketStream {
        PacketStream::new(self)
    }
    
    fn into_connection_stream(self) -> ConnectionStream {
        ConnectionStream::new(self)
    }
}

/// 客户端事件流 - 隐藏会话ID概念
pub struct ClientEventStream {
    inner: tokio::sync::broadcast::Receiver<crate::event::TransportEvent>,
}

impl ClientEventStream {
    /// 创建新的客户端事件流
    pub fn new(receiver: tokio::sync::broadcast::Receiver<crate::event::TransportEvent>) -> Self {
        Self { inner: receiver }
    }
    
    /// 接收下一个客户端事件
    pub async fn next(&mut self) -> Result<crate::event::ClientEvent, crate::error::TransportError> {
        loop {
            match self.inner.recv().await {
                Ok(transport_event) => {
                    // 转换为客户端事件，过滤掉不相关的事件
                    if let Some(client_event) = crate::event::ClientEvent::from_transport_event(transport_event) {
                        return Ok(client_event);
                    }
                    // 如果是不相关的事件，继续循环等待下一个事件
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    return Err(crate::error::TransportError::connection_error("Event stream closed", false));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // 事件流滞后，继续接收
                    continue;
                }
            }
        }
    }
    
    /// 尝试接收事件（非阻塞）
    pub fn try_next(&mut self) -> Result<Option<crate::event::ClientEvent>, crate::error::TransportError> {
        loop {
            match self.inner.try_recv() {
                Ok(transport_event) => {
                    // 转换为客户端事件，过滤掉不相关的事件
                    if let Some(client_event) = crate::event::ClientEvent::from_transport_event(transport_event) {
                        return Ok(Some(client_event));
                    }
                    // 如果是不相关的事件，继续循环
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    return Ok(None);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                    return Err(crate::error::TransportError::connection_error("Event stream closed", false));
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    // 事件流滞后，继续接收
                    continue;
                }
            }
        }
    }
} 