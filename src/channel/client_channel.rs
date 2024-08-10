use crate::packet::Packet;
use async_trait::async_trait;

#[async_trait]
pub trait ClientChannel {
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn receive(&mut self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>>;
}