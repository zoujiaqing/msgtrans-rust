/// 协议连接适配器
/// 
/// 将新的Connection trait包装成ProtocolAdapter trait，用于与现有Actor系统兼容

use async_trait::async_trait;
use crate::{
    SessionId,
    error::TransportError,
    command::ConnectionInfo,
    packet::Packet,
};
use super::{
    protocol::Connection,
    adapter::{ProtocolAdapter, AdapterStats, ProtocolConfig},
};

/// 协议连接适配器
/// 
/// 这个适配器将新的Connection trait包装成ProtocolAdapter trait
pub struct ProtocolConnectionAdapter {
    connection: Box<dyn Connection>,
    stats: AdapterStats,
}

impl ProtocolConnectionAdapter {
    /// 创建新的协议连接适配器
    pub fn new(connection: Box<dyn Connection>) -> Self {
        Self {
            connection,
            stats: AdapterStats::new(),
        }
    }
}

#[async_trait]
impl ProtocolAdapter for ProtocolConnectionAdapter {
    type Config = EmptyConfig;
    type Error = TransportError;
    
    async fn send(&mut self, packet: Packet) -> Result<(), Self::Error> {
        let packet_size = packet.payload.len();
        let result = self.connection.send(packet).await;
        
        if result.is_ok() {
            self.stats.record_packet_sent(packet_size);
        } else {
            self.stats.record_error();
        }
        
        result
    }
    
    async fn close(&mut self) -> Result<(), Self::Error> {
        self.connection.close().await
    }
    
    fn connection_info(&self) -> ConnectionInfo {
        self.connection.connection_info()
    }
    
    fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }
    
    fn stats(&self) -> AdapterStats {
        self.stats.clone()
    }
    
    fn session_id(&self) -> SessionId {
        self.connection.session_id()
    }
    
    fn set_session_id(&mut self, session_id: SessionId) {
        self.connection.set_session_id(session_id);
    }
    
    async fn flush(&mut self) -> Result<(), Self::Error> {
        // 大多数协议连接不需要显式flush
        Ok(())
    }
}

impl ProtocolConnectionAdapter {
    /// 获取事件流 - 事件驱动架构的核心
    pub fn subscribe_events(&self) -> Option<tokio::sync::broadcast::Receiver<crate::event::TransportEvent>> {
        self.connection.get_event_stream()
    }
}

/// 空配置类型，用于协议连接适配器
#[derive(Debug, Clone)]
pub struct EmptyConfig;

impl ProtocolConfig for EmptyConfig {
    fn validate(&self) -> Result<(), super::adapter::ConfigError> {
        Ok(())
    }
    
    fn default_config() -> Self {
        Self
    }
    
    fn merge(self, _other: Self) -> Self {
        self
    }
} 