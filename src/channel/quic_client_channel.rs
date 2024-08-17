use anyhow::Result;
use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler, OnClientMessageHandler,
};
use crate::packet::Packet;
use s2n_quic::client::Connect;
use s2n_quic::connection::Connection;
use s2n_quic::stream::ReceiveStream;
use s2n_quic::stream::SendStream;
use std::{path::Path, net::SocketAddr};
use bytes::Bytes;
use std::io;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;

pub struct QuicClientChannel {
    connection: Option<Connection>,
    receive_stream: Option<Arc<tokio::sync::Mutex<ReceiveStream>>>,
    send_stream: Option<Arc<tokio::sync::Mutex<SendStream>>>,
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
            receive_stream: None,
            send_stream: None,
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
        println!("Connecting ...");
        let client = s2n_quic::Client::builder()
            .with_tls(Path::new(self.cert_path))?
            .with_io("0.0.0.0:0")?
            .start()?;

        let addr: SocketAddr = format!("{}:{}", self.host, self.port).parse()?;
        let connect = Connect::new(addr).with_server_name("localhost");
        let mut connection = client.connect(connect).await?;
        println!("Connected!");

        connection.keep_alive(true)?;

        let stream = connection.open_bidirectional_stream().await?;
        let (receive_stream, send_stream) = stream.split();

        let receive_stream = Arc::new(tokio::sync::Mutex::new(receive_stream));
        let send_stream = Arc::new(tokio::sync::Mutex::new(send_stream));

        self.connection = Some(connection);
        self.receive_stream = Some(receive_stream.clone());
        self.send_stream = Some(send_stream.clone());

        if let Some(ref handler) = self.on_reconnect {
            let handler = handler.lock().await;
            (*handler)();  // 解引用并调用
        }

        let on_message = self.on_message.clone();
        let on_disconnect = self.on_disconnect.clone();
        let on_error = self.on_error.clone();

        // 启动接收任务
        tokio::spawn({
            let receive_stream = receive_stream.clone();
            async move {
                let mut buf = vec![0u8; 1024];
                loop {
                    let mut stream = receive_stream.lock().await;
                    match stream.read(&mut buf).await {
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
            }
        });

        Ok(())
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("channel.send()");

        if let Some(ref send_stream) = self.send_stream {
            let mut stream = send_stream.lock().await;
            let data = Bytes::from(packet.to_bytes());
            stream.write_all(&data).await?;
            println!("Data sent");

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