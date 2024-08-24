use std::sync::Arc;
use tokio::sync::Mutex;
use crate::packet::Packet;
use crate::channel::ClientChannel;
use crate::callbacks::{
    OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler,
};

pub struct MessageTransportClient<C: ClientChannel + Send + Sync> {
    channel: Option<Arc<Mutex<C>>>,
    reconnect_handler: Option<OnReconnectHandler>,
    disconnect_handler: Option<OnClientDisconnectHandler>,
    error_handler: Option<OnClientErrorHandler>,
    send_handler: Option<OnSendHandler>,
    message_handler: Option<OnClientMessageHandler>,
}

impl<C: ClientChannel + Send + Sync + 'static> MessageTransportClient<C> {
    pub fn new() -> Self {
        MessageTransportClient {
            channel: None,
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
        }
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            let mut channel_guard = channel.lock().await;
            match channel_guard.connect().await {
                Ok(_) => {
                    println!("Connected successfully!");

                    if let Some(handler) = &self.reconnect_handler {
                        let handler_guard = handler.lock().await;
                        handler_guard();
                    }

                    Ok(())
                }
                Err(e) => {
                    if let Some(handler) = &self.error_handler {
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

    pub async fn send(&self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            let mut channel_guard = channel.lock().await;
            channel_guard.send(packet).await
        } else {
            Err("No channel set".into())
        }
    }

    pub fn set_channel(&mut self, mut channel: C) {
        if let Some(ref handler) = self.reconnect_handler {
            channel.set_reconnect_handler(handler.clone());
        }
        if let Some(ref handler) = self.disconnect_handler {
            channel.set_disconnect_handler(handler.clone());
        }
        if let Some(ref handler) = self.error_handler {
            channel.set_error_handler(handler.clone());
        }
        if let Some(ref handler) = self.send_handler {
            channel.set_send_handler(handler.clone());
        }
        if let Some(ref handler) = self.message_handler {
            channel.set_message_handler(handler.clone());
        }

        self.channel = Some(Arc::new(Mutex::new(channel)));
    }

    pub fn set_reconnect_handler_handler(&mut self, handler: OnReconnectHandler) {
        self.reconnect_handler = Some(handler);
    }

    pub fn set_disconnect_handler(&mut self, handler: OnClientDisconnectHandler) {
        self.disconnect_handler = Some(handler);
    }

    pub fn set_error_handler(&mut self, handler: OnClientErrorHandler) {
        self.error_handler = Some(handler);
    }

    pub fn set_send_handler_handler(&mut self, handler: OnSendHandler) {
        self.send_handler = Some(handler);
    }

    pub fn set_message_handler(&mut self, handler: OnClientMessageHandler) {
        self.message_handler = Some(handler);
    }
}

impl<C: ClientChannel + Send + Sync> Clone for MessageTransportClient<C> {
    fn clone(&self) -> Self {
        MessageTransportClient {
            channel: self.channel.clone(),
            reconnect_handler: self.reconnect_handler.clone(),
            disconnect_handler: self.disconnect_handler.clone(),
            error_handler: self.error_handler.clone(),
            send_handler: self.send_handler.clone(),
            message_handler: self.message_handler.clone(),
        }
    }
}