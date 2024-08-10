use crate::packet::Packet;
use crate::channel::ClientChannel;
use std::sync::{Arc, RwLock};

pub struct MessageTransportClient<C: ClientChannel + Send + Sync> {
    channel: Option<Arc<RwLock<C>>>,
}

impl<C: ClientChannel + Send + Sync + 'static> MessageTransportClient<C> {
    pub fn new() -> Self {
        MessageTransportClient { channel: None }
    }

    pub fn set_channel(&mut self, channel: C) {
        self.channel = Some(Arc::new(RwLock::new(channel)));
    }

    pub async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            channel.write().unwrap().connect().await?;
            Ok(())
        } else {
            Err("No channel set".into())
        }
    }

    pub async fn send(&self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            channel.write().unwrap().send(packet).await?; // 使用写锁
        }
        Ok(())
    }

    pub async fn receive(&self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(channel) = &self.channel {
            return channel.write().unwrap().receive().await; // 使用写锁
        }
        Ok(None)
    }
}