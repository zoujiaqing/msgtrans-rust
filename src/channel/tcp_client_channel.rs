use super::ClientChannel;
use crate::callbacks::{
    OnClientDisconnectHandler, OnClientErrorHandler, OnReconnectHandler, OnSendHandler,
};
use crate::packet::Packet;
use bytes::BytesMut;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use std::io;

pub struct TcpClientChannel {
    stream: Option<TcpStream>,
    address: String,
    port: u16,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
}

impl TcpClientChannel {
    pub fn new(address: &str, port: u16) -> Self {
        TcpClientChannel {
            stream: None,
            address: address.to_string(),
            port,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for TcpClientChannel {
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

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match TcpStream::connect((&self.address[..], self.port)).await {
            Ok(stream) => {
                self.stream = Some(stream);
                if let Some(ref handler) = self.on_reconnect {
                    let handler = handler.lock().await;
                    handler();
                }
                Ok(())
            }
            Err(e) => {
                if let Some(ref handler) = self.on_error {
                    let handler = handler.lock().await;
                    handler(Box::new(e));
                }
                Err("Failed to connect".into())
            }
        }
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            // 发送数据并捕获可能的错误
            let send_result = stream.write_all(&packet.to_bytes()).await;
    
            // 调用 OnSendHandler 回调，并传递 Packet 和 Result
            if let Some(ref handler) = self.on_send {
                let handler = handler.lock().await;
    
                // 将 send_result 克隆一份给回调函数
                let send_result_for_handler = send_result
                    .as_ref() // 使用 as_ref 获取一个 &Result 而不是移动 send_result
                    .map(|_| ()) // 将 Ok() 映射为 ()
                    .map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>);
    
                handler(packet.clone(), send_result_for_handler);
            }
    
            // 直接返回 send_result，但需要映射 Ok() 的内容为 ()
            send_result.map(|_| ()).map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            // 如果连接没有建立，创建错误消息并调用 OnErrorHandler
            let err_msg: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established"));
    
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler(err_msg);
            }
    
            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }

    async fn receive(
        &mut self,
    ) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            let mut buf = BytesMut::with_capacity(1024);
            match stream.read_buf(&mut buf).await {
                Ok(n) if n > 0 => Ok(Some(Packet::from_bytes(&buf[..n]))),
                Ok(_) => Ok(None),
                Err(e) => {
                    if let Some(ref handler) = self.on_error {
                        let handler = handler.lock().await;
                        handler(Box::new(e));
                    }
                    Err("Failed to receive packet".into())
                }
            }
        } else {
            if let Some(ref handler) = self.on_error {
                let handler = handler.lock().await;
                handler("No connection established".into());
            }
            Err("No connection established".into())
        }
    }
}
