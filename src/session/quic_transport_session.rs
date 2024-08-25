use crate::callbacks::{
    OnMessageHandler, OnCloseHandler, OnSessionErrorHandler, OnSessionTimeoutHandler,
};
use crate::context::Context;
use crate::packet::{Packet, PacketHeader};
use bytes::{Buf, Bytes, BytesMut};
use s2n_quic::connection::Connection;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::sync::Arc;
use tokio::sync::Mutex;
use async_trait::async_trait;
use crate::session::TransportSession;

pub struct QuicTransportSession {
    connection: Arc<Mutex<Connection>>,
    receive_stream: Arc<Mutex<Option<ReceiveStream>>>,
    send_stream: Arc<Mutex<Option<SendStream>>>,
    id: usize,
    message_handler: Mutex<Option<Arc<Mutex<OnMessageHandler>>>>,
    close_handler: Mutex<Option<Arc<Mutex<OnCloseHandler>>>>,
    error_handler: Mutex<Option<Arc<Mutex<OnSessionErrorHandler>>>>,
    timeout_handler: Mutex<Option<Arc<Mutex<OnSessionTimeoutHandler>>>>,
}

impl QuicTransportSession {
    pub fn new(connection: Connection, id: usize) -> Arc<Self> {
        Arc::new(QuicTransportSession {
            connection: Arc::new(Mutex::new(connection)),
            receive_stream: Arc::new(Mutex::new(None)),
            send_stream: Arc::new(Mutex::new(None)),
            id,
            message_handler: Mutex::new(None),
            close_handler: Mutex::new(None),
            error_handler: Mutex::new(None),
            timeout_handler: Mutex::new(None),
        })
    }
}

#[async_trait]
impl TransportSession for QuicTransportSession {
    async fn send(self: Arc<Self>, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut stream_guard = self.send_stream.lock().await;
        if let Some(ref mut send_stream) = *stream_guard {
            let data = packet.to_bytes();
            send_stream.send(Bytes::from(data)).await?;
        } else {
            return Err("Send stream is not available".into());
        }
        Ok(())
    }
    
    async fn start_receiving(self: Arc<Self>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut connection_guard = self.connection.lock().await;
        if let Some(stream) = connection_guard.accept_bidirectional_stream().await? {
            let (receive_stream, send_stream) = stream.split();
            *self.receive_stream.lock().await = Some(receive_stream);
            *self.send_stream.lock().await = Some(send_stream);
        }

        if let Some(ref mut stream) = *self.receive_stream.lock().await {
            let mut buffer = BytesMut::new();
            while let Some(data) = stream.receive().await? {
                
                buffer.extend_from_slice(&data);

                while buffer.len() >= 16 {
                    // Check if we have enough data to parse the PacketHeader
                    let header = PacketHeader::from_bytes(&buffer[..16]);

                    // Check if the full packet is available
                    let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                    if buffer.len() < total_length {
                        break; // Not enough data, wait for more
                    }

                    // Parse the full packet
                    let packet = Packet::from_bytes(header, &buffer[16..total_length]);


                    if let Some(handler) = self.get_message_handler().await {
                        let context = Arc::new(Context::new(self.clone() as Arc<dyn TransportSession + Send + Sync>));
                        handler.lock().await(context, packet);
                    }

                    // Remove the parsed packet from the buffer
                    buffer.advance(total_length);
                }
            }
        } else {
            return Err("Receive stream is not available".into());
        }
    
        Ok(())
    }

    async fn close_session(self: Arc<Self>, context: Arc<Context>) {
        let connection = self.connection.lock().await;
        const MY_ERROR_CODE: u32 = 99;
        connection.close(MY_ERROR_CODE.into());

        if let Some(handler) = self.get_close_handler().await {
            handler.lock().await(context);
        }
    }

    fn id(&self) -> usize {
        self.id
    }

    async fn set_message_handler(self: Arc<Self>, handler: Arc<Mutex<OnMessageHandler>>) {
        let mut message_handler = self.message_handler.lock().await;
        *message_handler = Some(handler);
    }

    async fn get_message_handler(&self) -> Option<Arc<Mutex<OnMessageHandler>>> {
        let message_handler = self.message_handler.lock().await;
        message_handler.clone()
    }

    async fn set_close_handler(self: Arc<Self>, handler: Arc<Mutex<OnCloseHandler>>) {
        let mut close_handler = self.close_handler.lock().await;
        *close_handler = Some(handler);
    }

    async fn get_close_handler(&self) -> Option<Arc<Mutex<OnCloseHandler>>> {
        let close_handler = self.close_handler.lock().await;
        close_handler.clone()
    }

    async fn set_error_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionErrorHandler>>) {
        let mut error_handler = self.error_handler.lock().await;
        *error_handler = Some(handler);
    }

    async fn get_error_handler(&self) -> Option<Arc<Mutex<OnSessionErrorHandler>>> {
        let error_handler = self.error_handler.lock().await;
        error_handler.clone()
    }

    async fn set_timeout_handler(self: Arc<Self>, handler: Arc<Mutex<OnSessionTimeoutHandler>>) {
        let mut timeout_handler = self.timeout_handler.lock().await;
        *timeout_handler = Some(handler);
    }

    async fn get_timeout_handler(&self) -> Option<Arc<Mutex<OnSessionTimeoutHandler>>> {
        let timeout_handler = self.timeout_handler.lock().await;
        timeout_handler.clone()
    }
}