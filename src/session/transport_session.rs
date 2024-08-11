use crate::packet::Packet;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use async_trait::async_trait;

#[async_trait]
pub trait TransportSession: Send + Sync {
    async fn send_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn receive_packet(&mut self) -> Option<Packet>;
    async fn process_packet(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    async fn close(&mut self);
    fn id(&self) -> usize;
    fn set_message_handler(
        &mut self,
        handler: Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>,
    );
    fn get_message_handler(&self) -> Option<Arc<Mutex<Box<dyn Fn(Packet, Arc<Mutex<dyn TransportSession + Send + Sync>>) + Send + Sync>>>>;
}