use std::sync::Arc;
use tokio::sync::Mutex;
use crate::packet::Packet;
use crate::channel::ClientChannel;
use crate::callbacks::{
    OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler,
};

pub struct MessageTransportClient<C: ClientChannel + Send + Sync> {
    channel: Option<Arc<Mutex<C>>>,
    on_reconnect: Option<OnReconnectHandler>,
    on_disconnect: Option<OnClientDisconnectHandler>,
    on_error: Option<OnClientErrorHandler>,
    on_send: Option<OnSendHandler>,
    on_message_handler: Option<Arc<Mutex<dyn Fn(Packet) + Send + Sync>>>,
}

impl<C: ClientChannel + Send + Sync + 'static> MessageTransportClient<C> {
    pub fn new() -> Self {
        MessageTransportClient {
            channel: None,
            on_reconnect: None,
            on_disconnect: None,
            on_error: None,
            on_send: None,
            on_message_handler: None,
        }
    }

    pub fn set_channel(&mut self, channel: C) {
        self.channel = Some(Arc::new(Mutex::new(channel)));
    }

    pub fn set_on_message_handler<F>(&mut self, handler: F)
    where
        F: Fn(Packet) + Send + Sync + 'static,
    {
        self.on_message_handler = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_on_reconnect_handler<F>(&mut self, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_reconnect = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_on_disconnect_handler<F>(&mut self, handler: F)
    where
        F: Fn() + Send + Sync + 'static,
    {
        self.on_disconnect = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_on_error_handler<F>(&mut self, handler: F)
    where
        F: Fn(Box<dyn std::error::Error + Send + Sync>) + Send + Sync + 'static,
    {
        self.on_error = Some(Arc::new(Mutex::new(handler)));
    }

    pub fn set_on_send_handler<F>(&mut self, handler: F)
    where
        F: Fn(Packet) + Send + Sync + 'static,
    {
        self.on_send = Some(Arc::new(Mutex::new(handler)));
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            let mut channel_guard = channel.lock().await;
            match channel_guard.connect().await {
                Ok(_) => {
                    println!("11111");
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

    pub async fn send(&self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            if let Some(handler) = &self.on_send {
                let handler_guard = handler.lock().await;
                handler_guard(packet.clone());
            }
            match channel.lock().await.send(packet).await {
                Ok(_) => Ok(()),
                Err(e) => {
                    if let Some(handler) = &self.on_error {
                        let handler_guard = handler.lock().await;
                        handler_guard(e);
                    }
                    Err("Failed to send packet".into())
                }
            }
        } else {
            Err("No channel set".into())
        }
    }

    pub async fn receive(&self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            match channel.lock().await.receive().await {
                Ok(Some(packet)) => {
                    if let Some(handler) = &self.on_message_handler {
                        let handler_guard = handler.lock().await;
                        handler_guard(packet.clone());
                    }
                    Ok(Some(packet))
                }
                Ok(None) => Ok(None),
                Err(e) => {
                    if let Some(handler) = &self.on_error {
                        let handler_guard = handler.lock().await;
                        handler_guard(e);
                    }
                    Err("Failed to receive packet".into())
                }
            }
        } else {
            Ok(None)
        }
    }
}