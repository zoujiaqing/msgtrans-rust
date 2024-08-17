use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::Packet;
use s2n_quic::client::Connect;
use s2n_quic::connection::Connection;
use s2n_quic::stream::{ReceiveStream, SendStream};
use std::{path::Path, net::SocketAddr};
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::{Arc};
use tokio::sync::Mutex; // 使用 tokio 的异步 Mutex
use std::io;

pub struct QuicClientChannel {
    connection: Option<Arc<Mutex<Connection>>>,
    host: String,
    port: u16,
    cert_path: &'static str,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
    on_message: Option<OnClientMessageHandler>,
}

impl QuicClientChannel {
    pub fn new(host: &str, port: u16, cert_path: &'static str) -> Self {
        QuicClientChannel {
            connection: None,
            host: host.to_string(),
            port,
            cert_path,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
            on_message: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for QuicClientChannel {
    fn set_reconnect_handler(&mut self, handler: OnReconnectHandler) {
        self.on_reconnect = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: OnClientDisconnectHandler) {
        self.on_disconnect = Some(handler);
    }

    fn set_error_handler(&mut self, handler: OnClientErrorHandler) {
        self.on_error = Some(handler);
    }

    fn set_send_handler(&mut self, handler: OnSendHandler) {
        self.on_send = Some(handler);
    }

    fn set_on_message_handler(&mut self, handler: OnClientMessageHandler) {
        self.on_message = Some(handler);
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = s2n_quic::Client::builder()
            .with_tls(Path::new(self.cert_path))?
            .with_io("0.0.0.0:0")?
            .start()?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let connect = Connect::new(addr).with_server_name("localhost");
        let mut connection = client.connect(connect).await?;

        connection.keep_alive(true)?;

        // let connection = Arc::new(Mutex::new(connection));

        if let Some(ref handler) = self.on_reconnect {
            let handler = handler.lock().await;
            (*handler)();
        }
        
        let stream = connection.open_bidirectional_stream().await.unwrap();
        let (mut receive_stream, _) = stream.split();
        let on_message = self.on_message.clone();
        let on_disconnect = self.on_disconnect.clone();
        let on_error = self.on_error.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 1024];
            loop {
                match receive_stream.read(&mut buf).await {
                    Ok(n) if n > 0 => {
                        if let Some(ref handler) = on_message {
                            let packet = Packet::from_bytes(&buf[..n]);
                            let handler = handler.lock().await;
                            (*handler)(packet);
                        }
                    }
                    Ok(_) => {
                        if let Some(ref handler) = on_disconnect {
                            let handler = handler.lock().await;
                            (*handler)();
                        }
                        break;
                    }
                    Err(e) => {
                        if let Some(ref handler) = on_error {
                            let handler = handler.lock().await;
                            (*handler)(Box::new(e) as Box<dyn std::error::Error + Send + Sync>);
                        }
                        break;
                    }
                }
            }
        });

        self.connection = Some(Arc::new(Mutex::new(connection)));

        Ok(())
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {

        if let Some(ref connection) = self.connection {
            // 每次发送数据都打开一个新的流
            let mut conn = connection.lock().await;
            let mut stream = conn.open_bidirectional_stream().await?;
            let data = Bytes::from(packet.to_bytes());
            let (_, mut send_stream) = stream.split();
            send_stream.write_all(&data).await?;

            // 调用 OnSendHandler 回调，并传递 Packet 和 Result
            if let Some(ref handler) = self.on_send {
                let handler = handler.lock().await;
                (*handler)(packet.clone(), Ok(()));
            }

            Ok(())
        } else {
            let err_msg: Box<dyn std::error::Error + Send + Sync> = "No connection established".into();

            // 调用 OnErrorHandler 回调
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                (*handler)(err_msg);
            }

            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }

    async fn start_receiving(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())  // 方法保留但无操作
    }
}