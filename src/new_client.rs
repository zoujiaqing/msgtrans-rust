use std::sync::Arc;
use futures::{SinkExt, StreamExt};

use crate::transport::{Transport, ProtocolSpecific, TransportError, TransportControl};
use crate::packet::Packet;

/// 消息处理器类型
pub type MessageHandler = Arc<dyn Fn(Packet) + Send + Sync>;

/// 新的传输层客户端
pub struct TransportClient<T: ProtocolSpecific> {
    sender: Option<T::Sender>,
    control: Option<T::Control>,
    receiver: Option<T::Receiver>,
    message_handler: Option<MessageHandler>,
}

impl<T: ProtocolSpecific> TransportClient<T> {
    /// 创建新客户端
    pub fn new() -> Self {
        Self {
            sender: None,
            control: None,
            receiver: None,
            message_handler: None,
        }
    }

    /// 设置协议连接
    pub fn set_connection(&mut self, connection: T) {
        let transport = Transport::new(Arc::new(connection));
        let (sender, receiver, control) = transport.split();
        
        self.sender = Some(sender);
        self.control = Some(control);
        self.receiver = Some(receiver);
        
        // 如果消息处理器已设置，立即启动接收任务
        self.try_start_receiver_task();
    }

    /// 设置消息处理器
    pub fn set_message_handler<F>(&mut self, handler: F)
    where
        F: Fn(Packet) + Send + Sync + 'static,
    {
        self.message_handler = Some(Arc::new(handler));
        
        // 如果接收器已设置，立即启动接收任务
        self.try_start_receiver_task();
    }

    /// 尝试启动接收任务（当消息处理器和接收器都准备好时）
    fn try_start_receiver_task(&mut self) {
        // 只有当两者都存在且接收任务未启动时才启动
        if self.message_handler.is_some() && self.receiver.is_some() {
            if let (Some(handler), Some(receiver)) = (self.message_handler.clone(), self.receiver.take()) {
                println!("启动接收任务...");
                tokio::spawn(async move {
                    let mut receiver = receiver;
                    println!("接收任务已启动，开始监听消息...");
                    while let Some(result) = receiver.next().await {
                        match result {
                            Ok(packet) => {
                                println!("🎉 收到服务器回复! ID: {}, 载荷: {:?}", 
                                    packet.header.message_id, 
                                    String::from_utf8_lossy(&packet.payload)
                                );
                                handler(packet);
                            }
                            Err(e) => {
                                eprintln!("接收消息错误: {:?}", e);
                                break;
                            }
                        }
                    }
                    println!("接收任务结束");
                });
            }
        } else {
            println!("接收任务启动失败，handler: {}, receiver: {}", 
                self.message_handler.is_some(), 
                self.receiver.is_some()
            );
        }
    }

    /// 连接（启动消息处理）
    pub async fn connect(&mut self) -> Result<(), TransportError> {
        // 在新架构中，连接在set_connection时已完成
        if self.sender.is_none() {
            return Err(TransportError::NotConnected);
        }
        println!("Connected successfully!");
        Ok(())
    }

    /// 发送消息
    pub async fn send(&mut self, packet: Packet) -> Result<(), TransportError> {
        if let Some(sender) = &mut self.sender {
            sender.send(packet).await
                .map_err(|e| TransportError::SendError(e.to_string()))?;
            Ok(())
        } else {
            Err(TransportError::NotConnected)
        }
    }

    /// 获取控制接口
    pub fn control(&self) -> Option<&T::Control> {
        self.control.as_ref()
    }

    /// 关闭连接
    pub async fn close(&self) -> Result<(), TransportError> {
        if let Some(control) = &self.control {
            control.close().await
        } else {
            Err(TransportError::NotConnected)
        }
    }
}

impl<T: ProtocolSpecific> Default for TransportClient<T> {
    fn default() -> Self {
        Self::new()
    }
} 