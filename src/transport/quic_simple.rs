use std::sync::Arc;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::{mpsc};
use futures::{Sink, Stream};
use quinn::{Connection};
use bytes::BytesMut;

use crate::transport::{ProtocolSpecific, TransportControl, TransportError, SessionInfo};
use crate::packet::{Packet};

/// 简化的 QUIC 连接
pub struct QuicConnection {
    session_info: SessionInfo,
    connection: Arc<Connection>,
    command_tx: mpsc::Sender<QuicCommand>,
}

/// QUIC 命令
#[derive(Debug)]
enum QuicCommand {
    Send(Packet),
    Close,
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

        let (command_tx, mut command_rx) = mpsc::channel(1024);
        let connection_arc = Arc::new(connection);

        // 启动后台任务处理命令
        let conn_clone = connection_arc.clone();
        tokio::spawn(async move {
            // 开启双向流
            if let Ok((mut send_stream, _recv_stream)) = conn_clone.open_bi().await {
                while let Some(cmd) = command_rx.recv().await {
                    match cmd {
                        QuicCommand::Send(packet) => {
                            use tokio::io::AsyncWriteExt;
                            let data = packet.to_bytes();
                            let _ = send_stream.write_all(&data).await;
                        }
                        QuicCommand::Close => {
                            conn_clone.close(0u32.into(), b"Closed");
                            break;
                        }
                    }
                }
            }
        });

        Ok(Self {
            session_info,
            connection: connection_arc,
            command_tx,
        })
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
        // 简化实现，假设总是准备好
        Poll::Ready(Ok(()))
    }

    fn start_send(self: Pin<&mut Self>, packet: Packet) -> Result<(), Self::Error> {
        self.command_tx.try_send(QuicCommand::Send(packet))
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

/// QUIC 接收器 - 简化版本
#[derive(Clone)]
pub struct QuicReceiver {
    _placeholder: (),
}

impl QuicReceiver {
    fn new() -> Self {
        Self { _placeholder: () }
    }
}

impl Stream for QuicReceiver {
    type Item = Result<Packet, TransportError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // 简化实现，总是返回Pending
        Poll::Pending
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
        QuicReceiver::new()
    }

    fn create_control(&self) -> Self::Control {
        QuicControl::new(self.session_info.clone(), self.command_tx.clone())
    }
} 