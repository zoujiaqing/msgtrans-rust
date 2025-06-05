use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, broadcast};
use tokio::io::AsyncWriteExt;
use futures::{Sink, Stream};
use quinn::{Connection, SendStream, RecvStream};
use bytes::BytesMut;

use crate::transport::{ProtocolSpecific, TransportControl, TransportError, SessionInfo};
use crate::packet::{Packet, PacketHeader};

/// QUIC 连接包装
pub struct QuicConnection {
    connection: Connection,
    session_info: SessionInfo,
    command_tx: mpsc::Sender<QuicCommand>,
    event_rx: broadcast::Receiver<QuicEvent>,
}

/// QUIC 内部命令
#[derive(Debug)]
enum QuicCommand {
    Send { 
        packet: Packet, 
        result_tx: tokio::sync::oneshot::Sender<Result<(), TransportError>> 
    },
    Close,
}

/// QUIC 事件
#[derive(Debug, Clone)]
pub enum QuicEvent {
    PacketReceived(Packet),
    Error(String),
    Disconnected,
}

impl QuicConnection {
    pub async fn new(connection: Connection, session_id: usize) -> Result<Self, TransportError> {
        let local_addr = connection.local_ip()
            .map(|ip| std::net::SocketAddr::new(ip, 0))
            .unwrap_or_else(|| "0.0.0.0:0".parse().unwrap());
        let remote_addr = connection.remote_address();
        
        let session_info = SessionInfo {
            session_id,
            protocol: "QUIC".to_string(),
            peer_address: remote_addr.to_string(),
        };

        let (command_tx, command_rx) = mpsc::channel(1024);
        let (event_tx, event_rx) = broadcast::channel(1024);

        // 启动 QUIC Actor
        let actor = QuicActor {
            connection: connection.clone(),
            session_id,
            command_rx,
            event_tx,
        };
        tokio::spawn(actor.run());

        Ok(Self {
            connection,
            session_info,
            command_tx,
            event_rx,
        })
    }
}

/// QUIC Actor - 管理所有 QUIC 操作
struct QuicActor {
    connection: Connection,
    session_id: usize,
    command_rx: mpsc::Receiver<QuicCommand>,
    event_tx: broadcast::Sender<QuicEvent>,
}

impl QuicActor {
    async fn run(mut self) {
        // 建立双向流
        let (send_stream, recv_stream) = match self.connection.open_bi().await {
            Ok(streams) => streams,
            Err(e) => {
                let _ = self.event_tx.send(QuicEvent::Error(
                    format!("Failed to open stream: {}", e)
                ));
                return;
            }
        };

        let mut send_stream = Some(send_stream);
        let mut recv_stream = Some(recv_stream);

        loop {
            tokio::select! {
                // 处理命令
                cmd = self.command_rx.recv() => {
                    match cmd {
                        Some(QuicCommand::Send { packet, result_tx }) => {
                            let result = self.handle_send(&mut send_stream, packet).await;
                            let _ = result_tx.send(result);
                        }
                        Some(QuicCommand::Close) => break,
                        None => break,
                    }
                }
                
                // 接收数据
                result = Self::receive_packet_static(&mut recv_stream) => {
                    match result {
                        Ok(Some(packet)) => {
                            let _ = self.event_tx.send(QuicEvent::PacketReceived(packet));
                        }
                        Ok(None) => {
                            let _ = self.event_tx.send(QuicEvent::Disconnected);
                            break;
                        }
                        Err(e) => {
                            let _ = self.event_tx.send(QuicEvent::Error(e.to_string()));
                            break;
                        }
                    }
                }
            }
        }

        // 清理
        self.connection.close(0u32.into(), b"Session closed");
    }

    async fn handle_send(
        &self, 
        send_stream: &mut Option<SendStream>, 
        packet: Packet
    ) -> Result<(), TransportError> {
        if let Some(stream) = send_stream {
            let data = packet.to_bytes();
            stream.write_all(&data).await
                .map_err(|e| TransportError::SendError(e.to_string()))?;
            Ok(())
        } else {
            Err(TransportError::ConnectionClosed)
        }
    }

    async fn receive_packet_static(
        recv_stream: &mut Option<RecvStream>
    ) -> Result<Option<Packet>, TransportError> {
        if let Some(stream) = recv_stream {
            let mut buffer = BytesMut::new();
            let mut temp_buffer = [0u8; 1024];
            
            match stream.read(&mut temp_buffer).await {
                Ok(Some(n)) => {
                    if n > 0 {
                        buffer.extend_from_slice(&temp_buffer[..n]);
                        
                        // 简化的包解析（实际应该处理分包情况）
                        if buffer.len() >= 16 {
                            let header = PacketHeader::from_bytes(&buffer[..16]);
                            let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                            
                            if buffer.len() >= total_length {
                                let packet = Packet::by_header_from_bytes(header, &buffer[16..total_length]);
                                return Ok(Some(packet));
                            }
                        }
                        
                        // 数据不完整，继续等待
                        Ok(None)
                    } else {
                        // 0 字节表示流关闭
                        Ok(None)
                    }
                }
                Ok(None) => Ok(None),
                Err(e) => Err(TransportError::ReceiveError(e.to_string())),
            }
        } else {
            Ok(None)
        }
    }
}

/// QUIC 发送器
#[derive(Clone)]
pub struct QuicSender {
    command_tx: mpsc::Sender<QuicCommand>,
}

impl QuicSender {
    fn new(command_tx: mpsc::Sender<QuicCommand>) -> Self {
        Self { command_tx }
    }
}

impl Sink<Packet> for QuicSender {
    type Error = TransportError;

    fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // mpsc::Sender 没有 poll_ready，我们简化为总是ready
        if self.command_tx.is_closed() {
            Poll::Ready(Err(TransportError::ChannelClosed))
        } else {
            Poll::Ready(Ok(()))
        }
    }

    fn start_send(self: Pin<&mut Self>, packet: Packet) -> Result<(), Self::Error> {
        let (result_tx, _result_rx) = tokio::sync::oneshot::channel();
        
        self.command_tx.try_send(QuicCommand::Send { packet, result_tx })
            .map_err(|_| TransportError::ChannelFull)?;
        
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = self.command_tx.try_send(QuicCommand::Close);
        Poll::Ready(Ok(()))
    }
}

/// QUIC 接收器
pub struct QuicReceiver {
    pub event_rx: broadcast::Receiver<QuicEvent>,
}

impl Clone for QuicReceiver {
    fn clone(&self) -> Self {
        Self {
            event_rx: self.event_rx.resubscribe(),
        }
    }
}

impl QuicReceiver {
    fn new(event_rx: broadcast::Receiver<QuicEvent>) -> Self {
        Self { event_rx }
    }
}

impl Stream for QuicReceiver {
    type Item = Result<Packet, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // 使用 recv() 方法的 Future
        use std::future::Future;
        let mut recv_future = Box::pin(self.event_rx.recv());
        
        match recv_future.as_mut().poll(cx) {
            Poll::Ready(Ok(QuicEvent::PacketReceived(packet))) => {
                Poll::Ready(Some(Ok(packet)))
            }
            Poll::Ready(Ok(QuicEvent::Error(e))) => {
                Poll::Ready(Some(Err(TransportError::ProtocolError(e))))
            }
            Poll::Ready(Ok(QuicEvent::Disconnected)) => {
                Poll::Ready(None)
            }
            Poll::Ready(Err(_)) => Poll::Ready(None), // Lagged, 考虑重连
            Poll::Pending => Poll::Pending,
        }
    }
}

/// QUIC 控制器
#[derive(Clone)]
pub struct QuicControl {
    session_info: SessionInfo,
    command_tx: mpsc::Sender<QuicCommand>,
}

impl QuicControl {
    fn new(session_info: SessionInfo, command_tx: mpsc::Sender<QuicCommand>) -> Self {
        Self { session_info, command_tx }
    }
}

impl TransportControl for QuicControl {
    fn session_info(&self) -> &SessionInfo {
        &self.session_info
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.command_tx.send(QuicCommand::Close).await
            .map_err(|_| TransportError::ChannelClosed)?;
        Ok(())
    }
}

/// 实现协议特化
impl ProtocolSpecific for QuicConnection {
    type Sender = QuicSender;
    type Receiver = QuicReceiver;
    type Control = QuicControl;

    fn create_sender(&self) -> Self::Sender {
        QuicSender::new(self.command_tx.clone())
    }

    fn create_receiver(&self) -> Self::Receiver {
        QuicReceiver::new(self.event_rx.resubscribe())
    }

    fn create_control(&self) -> Self::Control {
        QuicControl::new(self.session_info.clone(), self.command_tx.clone())
    }
} 