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
        F: Fn(Packet, Result<(), Box<dyn std::error::Error + Send + Sync>>) + Send + Sync + 'static,
    {
        self.on_send = Some(Arc::new(Mutex::new(handler)));
    }

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

                    // 启动接收任务
                    let client_clone = self.clone(); // 复制客户端的引用
                    tokio::spawn(async move {
                        client_clone.receive_loop().await;
                    });

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
            let channel_clone = Arc::clone(&channel);
            let packet_clone = packet.clone();
            let on_send_handler = self.on_send.clone();
            let error_handler = self.on_error.clone();
    
            tokio::spawn(async move {
                // 获取锁，发送数据，释放锁
                let result = {
                    let mut channel_guard = channel_clone.lock().await;
                    channel_guard.send(packet_clone.clone()).await
                };
    
                // 处理发送结果
                if let Some(handler) = on_send_handler {
                    let handler_guard = handler.lock().await;
    
                    // 将 `Result<&(), Box<dyn Error>>` 转换为 `Result<(), Box<dyn Error>>`
                    let result_for_callback: Result<(), Box<dyn std::error::Error + Send + Sync>> = match &result {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.to_string().into()), // 将错误信息转为 Box<dyn Error>
                    };
    
                    handler_guard(packet_clone, result_for_callback);
                }
    
                // 如果有错误，处理错误
                if let Err(e) = result {
                    if let Some(handler) = error_handler {
                        let handler_guard = handler.lock().await;
                        handler_guard(e);
                    }
                }
            });
    
            Ok(())
        } else {
            Err("No channel set".into())
        }
    }
    
    async fn receive_loop(self) {
        while let Some(channel) = &self.channel {
            let packet_result = {
                // 获取锁，接收数据，然后释放锁
                let mut channel_guard = channel.lock().await;
                channel_guard.receive().await
            };
    
            match packet_result {
                Ok(Some(packet)) => {
                    if let Some(handler) = &self.on_message_handler {
                        let handler_guard = handler.lock().await;
                        handler_guard(packet);
                    }
                }
                Ok(None) => {
                    println!("Connection closed by server.");
                    break; // 连接可能已经关闭，退出循环
                }
                Err(e) => {
                    println!("Error in receiving: {:?}", e);
                    if let Some(handler) = &self.on_error {
                        let handler_guard = handler.lock().await;
                        handler_guard(e);
                    }
                    break;
                }
            }
        }
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
            on_message_handler: self.on_message_handler.clone(),
        }
    }
}