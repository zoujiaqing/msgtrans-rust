use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, broadcast};
use futures::{Sink, Stream, SinkExt, StreamExt};
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream, tungstenite::Message};
use tokio::net::TcpStream;
use std::net::SocketAddr;

use crate::packet::{Packet, PacketHeader};
use super::{ProtocolSpecific, TransportError, TransportControl, SessionInfo};

/// WebSocket 连接结构
pub struct WebSocketConnection {
    session_info: SessionInfo,
    command_tx: mpsc::Sender<WebSocketCommand>,
    event_rx: broadcast::Receiver<WebSocketEvent>,
}

/// WebSocket 命令
enum WebSocketCommand {
    Send { 
        packet: Packet, 
        result_tx: tokio::sync::oneshot::Sender<Result<(), TransportError>> 
    },
    Close,
}

/// WebSocket 事件
#[derive(Clone, Debug)]
pub enum WebSocketEvent {
    PacketReceived(Packet),
    Error(String),
    Disconnected,
}

impl WebSocketConnection {
    pub async fn new(ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>, session_id: usize, peer_addr: SocketAddr) -> Result<Self, TransportError> {
        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = broadcast::channel(100);
        
        let session_info = SessionInfo {
            session_id,
            protocol: "WebSocket".to_string(),
            peer_address: peer_addr.to_string(),
        };
        
        // 启动Actor
        let actor = WebSocketActor {
            ws_stream,
            session_id,
            command_rx,
            event_tx,
        };
        tokio::spawn(actor.run());
        
        Ok(Self {
            session_info,
            command_tx,
            event_rx,
        })
    }
    
    pub async fn connect(url: &str, session_id: usize) -> Result<Self, TransportError> {
        let (ws_stream, _) = tokio_tungstenite::connect_async(url).await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        
        // 从URL中提取地址信息（简化处理）
        let peer_addr = "0.0.0.0:0".parse().unwrap(); // 实际应该从URL解析
        
        Self::new(ws_stream, session_id, peer_addr).await
    }
}

/// WebSocket Actor
struct WebSocketActor {
    ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    session_id: usize,
    command_rx: mpsc::Receiver<WebSocketCommand>,
    event_tx: broadcast::Sender<WebSocketEvent>,
}

impl WebSocketActor {
    async fn run(self) {
        let WebSocketActor { mut ws_stream, session_id: _, mut command_rx, event_tx } = self;
        
        loop {
            tokio::select! {
                // 处理命令
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(WebSocketCommand::Send { packet, result_tx }) => {
                            let result = Self::handle_send(&mut ws_stream, packet).await;
                            let _ = result_tx.send(result);
                        }
                        Some(WebSocketCommand::Close) => {
                            let _ = ws_stream.close(None).await;
                            break;
                        }
                        None => break,
                    }
                }
                
                // 接收数据
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Binary(data))) => {
                            println!("WebSocket收到Binary消息，长度: {}", data.len());
                            match Self::parse_packet(&data) {
                                Ok(packet) => {
                                    println!("解析到数据包，消息ID: {}, 载荷长度: {}", packet.header.message_id, packet.payload.len());
                                    match event_tx.send(WebSocketEvent::PacketReceived(packet)) {
                                        Ok(receiver_count) => {
                                            println!("✅ 消息已发送到broadcast通道，接收器数量: {}", receiver_count);
                                        }
                                        Err(e) => {
                                            println!("❌ 发送到broadcast通道失败: {:?}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    println!("解析数据包失败: {}", e);
                                    let _ = event_tx.send(WebSocketEvent::Error(format!("Parse error: {}", e)));
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            println!("WebSocket收到关闭消息");
                            let _ = event_tx.send(WebSocketEvent::Disconnected);
                            break;
                        }
                        Some(Ok(msg_type)) => {
                            println!("WebSocket收到其他类型消息: {:?}", msg_type);
                            // 忽略其他类型的消息（Text, Ping, Pong等）
                        }
                        Some(Err(e)) => {
                            println!("WebSocket接收错误: {}", e);
                            let _ = event_tx.send(WebSocketEvent::Error(e.to_string()));
                            break;
                        }
                        None => {
                            println!("WebSocket连接已断开");
                            let _ = event_tx.send(WebSocketEvent::Disconnected);
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_send(
        ws_stream: &mut WebSocketStream<MaybeTlsStream<TcpStream>>, 
        packet: Packet
    ) -> Result<(), TransportError> {
        let data = packet.to_bytes().to_vec();
        ws_stream.send(Message::Binary(data)).await
            .map_err(|e| TransportError::SendError(e.to_string()))?;
        Ok(())
    }

    fn parse_packet(data: &[u8]) -> Result<Packet, TransportError> {
        if data.len() < 16 {
            return Err(TransportError::ProtocolError("Packet too short".to_string()));
        }
        
        let header = PacketHeader::from_bytes(&data[..16]);
        let total_length = 16 + header.extend_length as usize + header.message_length as usize;
        
        if data.len() >= total_length {
            Ok(Packet::by_header_from_bytes(header, &data[16..total_length]))
        } else {
            Err(TransportError::ProtocolError("Incomplete packet".to_string()))
        }
    }
}

/// WebSocket 发送器
#[derive(Clone)]
pub struct WebSocketSender {
    command_tx: mpsc::Sender<WebSocketCommand>,
}

impl WebSocketSender {
    fn new(command_tx: mpsc::Sender<WebSocketCommand>) -> Self {
        Self { command_tx }
    }
}

impl Sink<Packet> for WebSocketSender {
    type Error = TransportError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.command_tx.is_closed() {
            Poll::Ready(Err(TransportError::ChannelClosed))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn start_send(self: Pin<&mut Self>, packet: Packet) -> Result<(), Self::Error> {
        let (result_tx, _result_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.try_send(WebSocketCommand::Send { packet, result_tx })
            .map_err(|_| TransportError::ChannelFull)?;
        
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = self.command_tx.try_send(WebSocketCommand::Close);
        Poll::Ready(Ok(()))
    }
}

/// WebSocket 接收器
pub struct WebSocketReceiver {
    pub event_rx: broadcast::Receiver<WebSocketEvent>,
}

impl Clone for WebSocketReceiver {
    fn clone(&self) -> Self {
        Self {
            event_rx: self.event_rx.resubscribe(),
        }
    }
}

impl WebSocketReceiver {
    fn new(event_rx: broadcast::Receiver<WebSocketEvent>) -> Self {
        Self { event_rx }
    }
}



impl Stream for WebSocketReceiver {
    type Item = Result<Packet, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // 简化实现：使用try_recv避免复杂的Future状态管理
        match self.event_rx.try_recv() {
            Ok(WebSocketEvent::PacketReceived(packet)) => {
                Poll::Ready(Some(Ok(packet)))
            }
            Ok(WebSocketEvent::Error(e)) => {
                Poll::Ready(Some(Err(TransportError::ProtocolError(e))))
            }
            Ok(WebSocketEvent::Disconnected) => {
                Poll::Ready(None)
            }
            Err(_) => {
                Poll::Pending
            }
        }
    }
}

/// WebSocket 控制器
#[derive(Clone)]
pub struct WebSocketControl {
    session_info: SessionInfo,
    command_tx: mpsc::Sender<WebSocketCommand>,
}

impl WebSocketControl {
    fn new(session_info: SessionInfo, command_tx: mpsc::Sender<WebSocketCommand>) -> Self {
        Self { session_info, command_tx }
    }
}

impl TransportControl for WebSocketControl {
    fn session_info(&self) -> &SessionInfo {
        &self.session_info
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.command_tx.send(WebSocketCommand::Close).await
            .map_err(|_| TransportError::ChannelClosed)?;
        Ok(())
    }
}

/// 实现协议特化
impl ProtocolSpecific for WebSocketConnection {
    type Sender = WebSocketSender;
    type Receiver = WebSocketReceiver;
    type Control = WebSocketControl;

    fn create_sender(&self) -> Self::Sender {
        WebSocketSender::new(self.command_tx.clone())
    }

    fn create_receiver(&self) -> Self::Receiver {
        WebSocketReceiver::new(self.event_rx.resubscribe())
    }

    fn create_control(&self) -> Self::Control {
        WebSocketControl::new(self.session_info.clone(), self.command_tx.clone())
    }
} 