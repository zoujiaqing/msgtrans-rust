use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc, broadcast};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use futures::{Sink, Stream};
use bytes::BytesMut;
use std::net::SocketAddr;

use crate::packet::{Packet, PacketHeader};
use super::{ProtocolSpecific, TransportError, TransportControl, SessionInfo};

/// TCP 连接结构
pub struct TcpConnection {
    session_info: SessionInfo,
    command_tx: mpsc::Sender<TcpCommand>,
    event_rx: broadcast::Receiver<TcpEvent>,
}

/// TCP 命令
enum TcpCommand {
    Send { 
        packet: Packet, 
        result_tx: tokio::sync::oneshot::Sender<Result<(), TransportError>> 
    },
    Close,
}

/// TCP 事件
#[derive(Clone, Debug)]
pub enum TcpEvent {
    PacketReceived(Packet),
    Error(String),
    Disconnected,
}

impl TcpConnection {
    pub async fn new(stream: TcpStream, session_id: usize) -> Result<Self, TransportError> {
        let peer_addr = stream.peer_addr()
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        
        let (command_tx, command_rx) = mpsc::channel(100);
        let (event_tx, event_rx) = broadcast::channel(100);
        
        let session_info = SessionInfo {
            session_id,
            protocol: "TCP".to_string(),
            peer_address: peer_addr.to_string(),
        };
        
        // 启动Actor
        let actor = TcpActor {
            stream,
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
    
    pub async fn connect(addr: SocketAddr, session_id: usize) -> Result<Self, TransportError> {
        let stream = TcpStream::connect(addr).await
            .map_err(|e| TransportError::ConnectionError(e.to_string()))?;
        Self::new(stream, session_id).await
    }
}

/// TCP Actor
struct TcpActor {
    stream: TcpStream,
    session_id: usize,
    command_rx: mpsc::Receiver<TcpCommand>,
    event_tx: broadcast::Sender<TcpEvent>,
}

impl TcpActor {
    async fn run(self) {
        let TcpActor { stream, session_id: _, mut command_rx, event_tx } = self;
        let (mut read_half, mut write_half) = stream.into_split();
        let mut buffer = BytesMut::with_capacity(4096);
        
        loop {
            tokio::select! {
                // 处理命令
                cmd = command_rx.recv() => {
                    match cmd {
                        Some(TcpCommand::Send { packet, result_tx }) => {
                            let result = Self::handle_send(&mut write_half, packet).await;
                            let _ = result_tx.send(result);
                        }
                        Some(TcpCommand::Close) => break,
                        None => break,
                    }
                }
                
                // 接收数据
                result = read_half.read_buf(&mut buffer) => {
                    match result {
                        Ok(0) => {
                            let _ = event_tx.send(TcpEvent::Disconnected);
                            break;
                        }
                        Ok(n) => {
                            println!("TCP收到{}字节数据，缓冲区大小: {}", n, buffer.len());
                            // 处理缓冲区中的完整包
                            while let Some(packet) = Self::try_parse_packet(&mut buffer) {
                                println!("TCP解析到数据包，消息ID: {}, 载荷长度: {}", packet.header.message_id, packet.payload.len());
                                match event_tx.send(TcpEvent::PacketReceived(packet)) {
                                    Ok(receiver_count) => {
                                        println!("✅ TCP消息已发送到broadcast通道，接收器数量: {}", receiver_count);
                                    }
                                    Err(e) => {
                                        println!("❌ TCP发送到broadcast通道失败: {:?}", e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let _ = event_tx.send(TcpEvent::Error(e.to_string()));
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn handle_send(
        write_half: &mut tokio::net::tcp::OwnedWriteHalf, 
        packet: Packet
    ) -> Result<(), TransportError> {
        let data = packet.to_bytes();
        write_half.write_all(&data).await
            .map_err(|e| TransportError::SendError(e.to_string()))?;
        Ok(())
    }

    fn try_parse_packet(buffer: &mut BytesMut) -> Option<Packet> {
        if buffer.len() < 16 {
            return None; // 头部不完整
        }
        
        let header = PacketHeader::from_bytes(&buffer[..16]);
        let total_length = 16 + header.extend_length as usize + header.message_length as usize;
        
        if buffer.len() >= total_length {
            let packet_data = buffer.split_to(total_length);
            Some(Packet::by_header_from_bytes(header, &packet_data[16..total_length]))
        } else {
            None // 包体不完整
        }
    }
}

/// TCP 发送器
#[derive(Clone)]
pub struct TcpSender {
    command_tx: mpsc::Sender<TcpCommand>,
}

impl TcpSender {
    fn new(command_tx: mpsc::Sender<TcpCommand>) -> Self {
        Self { command_tx }
    }
}

impl Sink<Packet> for TcpSender {
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
        
        self.command_tx.try_send(TcpCommand::Send { packet, result_tx })
            .map_err(|_| TransportError::ChannelFull)?;
        
        Ok(())
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let _ = self.command_tx.try_send(TcpCommand::Close);
        Poll::Ready(Ok(()))
    }
}

/// TCP 接收器
pub struct TcpReceiver {
    pub event_rx: broadcast::Receiver<TcpEvent>,
}

impl Clone for TcpReceiver {
    fn clone(&self) -> Self {
        Self {
            event_rx: self.event_rx.resubscribe(),
        }
    }
}

impl TcpReceiver {
    fn new(event_rx: broadcast::Receiver<TcpEvent>) -> Self {
        Self { event_rx }
    }
}

impl Stream for TcpReceiver {
    type Item = Result<Packet, TransportError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        use std::future::Future;
        let mut recv_future = Box::pin(self.event_rx.recv());
        
        match recv_future.as_mut().poll(cx) {
            Poll::Ready(Ok(TcpEvent::PacketReceived(packet))) => {
                println!("🎉 TCP Stream收到数据包! ID: {}, 载荷: {:?}", 
                    packet.header.message_id, 
                    String::from_utf8_lossy(&packet.payload)
                );
                Poll::Ready(Some(Ok(packet)))
            }
            Poll::Ready(Ok(TcpEvent::Error(e))) => {
                println!("❌ TCP Stream收到错误: {}", e);
                Poll::Ready(Some(Err(TransportError::ProtocolError(e))))
            }
            Poll::Ready(Ok(TcpEvent::Disconnected)) => {
                println!("TCP连接已断开");
                Poll::Ready(None)
            }
            Poll::Ready(Err(_)) => {
                println!("TCP broadcast接收滞后");
                Poll::Ready(None)
            }, // Lagged
            Poll::Pending => Poll::Pending,
        }
    }
}

/// TCP 控制器
#[derive(Clone)]
pub struct TcpControl {
    session_info: SessionInfo,
    command_tx: mpsc::Sender<TcpCommand>,
}

impl TcpControl {
    fn new(session_info: SessionInfo, command_tx: mpsc::Sender<TcpCommand>) -> Self {
        Self { session_info, command_tx }
    }
}

impl TransportControl for TcpControl {
    fn session_info(&self) -> &SessionInfo {
        &self.session_info
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.command_tx.send(TcpCommand::Close).await
            .map_err(|_| TransportError::ChannelClosed)?;
        Ok(())
    }
}

/// 实现协议特化
impl ProtocolSpecific for TcpConnection {
    type Sender = TcpSender;
    type Receiver = TcpReceiver;
    type Control = TcpControl;

    fn create_sender(&self) -> Self::Sender {
        TcpSender::new(self.command_tx.clone())
    }

    fn create_receiver(&self) -> Self::Receiver {
        TcpReceiver::new(self.event_rx.resubscribe())
    }

    fn create_control(&self) -> Self::Control {
        TcpControl::new(self.session_info.clone(), self.command_tx.clone())
    }
} 