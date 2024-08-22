use std::sync::Arc;
use tokio::sync::Mutex;
use crate::packet::Packet;
use crate::channel::ClientChannel;
use crate::callbacks::{
    OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler,
};

pub struct MessageTransportClient<C: ClientChannel + Send + Sync> {
    channel: Option<Arc<Mutex<C>>>,  // 使用 Arc<Mutex<C>>
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
    on_message: Option<OnClientMessageHandler>,
}

impl<C: ClientChannel + Send + Sync + 'static> MessageTransportClient<C> {
    pub fn new() -> Self {
        MessageTransportClient {
            channel: None,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
            on_message: None,
        }
    }

    // 连接服务器并启动接收任务
    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            let mut channel_guard = channel.lock().await;
            match channel_guard.connect().await {
                Ok(_) => {
                    println!("Connected successfully!");

                    if let Some(handler) = &self.on_reconnect {
                        let handler_guard = handler.lock().await;
                        handler_guard();
                    }

                    Ok(())
                }
                Err(e) => {
                    if let Some(handler) = &self.on_error {
                        let handler_guard = handler.lock().await;
                        handler_guard(e);
                    }
                    Err("Failed to connect".into())
                }
            }
        } else {
            Err("No channel set".into())
        }
    }

    // 直接通过 ClientChannel 发送数据
    pub async fn send(&self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("sending..");
        if let Some(channel) = &self.channel {
            println!("sending..111");
            let mut channel_guard = channel.lock().await;
            println!("sent!");
            channel_guard.send(packet).await
        } else {
            Err("No channel set".into())
        }
    }

    // 设置 channel 并传递回调
    pub fn set_channel(&mut self, mut channel: C) {
        if let Some(ref handler) = self.on_reconnect {
            channel.set_reconnect_handler(handler.clone());
        }
        if let Some(ref handler) = self.on_disconnect {
            channel.set_disconnect_handler(handler.clone());
        }
        if let Some(ref handler) = self.on_error {
            channel.set_error_handler(handler.clone());
        }
        if let Some(ref handler) = self.on_send {
            channel.set_send_handler(handler.clone());
        }
        if let Some(ref handler) = self.on_message {
            channel.set_on_message_handler(handler.clone());
        }

        self.channel = Some(Arc::new(Mutex::new(channel)));
    }

    pub fn set_on_reconnect_handler(&mut self, handler: OnReconnectHandler) {
        self.on_reconnect = Some(handler);
    }

    pub fn set_on_disconnect_handler(&mut self, handler: OnClientDisconnectHandler) {
        self.on_disconnect = Some(handler);
    }

    pub fn set_on_error_handler(&mut self, handler: OnClientErrorHandler) {
        self.on_error = Some(handler);
    }

    pub fn set_on_send_handler(&mut self, handler: OnSendHandler) {
        self.on_send = Some(handler);
    }

    pub fn set_on_message_handler(&mut self, handler: OnClientMessageHandler) {
        self.on_message = Some(handler);
    }
}

impl<C: ClientChannel + Send + Sync> Clone for MessageTransportClient<C> {
    fn clone(&self) -> Self {
        MessageTransportClient {
            channel: self.channel.clone(),
            on_reconnect: self.on_reconnect.clone(),
            on_disconnect: self.on_disconnect.clone(),
            on_error: self.on_error.clone(),
            on_send: self.on_send.clone(),
            on_message: self.on_message.clone(),
        }
    }
}