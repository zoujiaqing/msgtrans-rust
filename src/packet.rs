/// 统一架构数据包定义
/// 
/// 为统一架构设计的简化、高效的数据包格式

use bytes::BytesMut;
use serde::{Serialize, Deserialize};

/// 数据包类型枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum PacketType {
    /// 心跳包
    Heartbeat = 0,
    /// 数据消息
    Data = 1,
    /// 控制消息
    Control = 2,
    /// 错误消息
    Error = 3,
    /// 认证消息
    Auth = 4,
    /// 回显消息（用于测试）
    Echo = 255,
}

impl From<u8> for PacketType {
    fn from(value: u8) -> Self {
        match value {
            0 => PacketType::Heartbeat,
            1 => PacketType::Data,
            2 => PacketType::Control,
            3 => PacketType::Error,
            4 => PacketType::Auth,
            255 => PacketType::Echo,
            _ => PacketType::Data, // 默认为数据类型
        }
    }
}

impl From<PacketType> for u8 {
    fn from(packet_type: PacketType) -> Self {
        packet_type as u8
    }
}

/// 统一架构数据包
/// 
/// 简化的数据包结构，专为统一架构设计
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedPacket {
    /// 数据包类型
    pub packet_type: PacketType,
    /// 消息ID（用于请求-响应匹配）
    pub message_id: u32,
    /// 负载数据
    pub payload: BytesMut,
}

impl UnifiedPacket {
    /// 创建新的数据包
    pub fn new(packet_type: PacketType, message_id: u32, payload: impl Into<BytesMut>) -> Self {
        Self {
            packet_type,
            message_id,
            payload: payload.into(),
        }
    }
    
    /// 创建数据消息包
    pub fn data(message_id: u32, payload: impl Into<BytesMut>) -> Self {
        Self::new(PacketType::Data, message_id, payload)
    }
    
    /// 创建控制消息包
    pub fn control(message_id: u32, payload: impl Into<BytesMut>) -> Self {
        Self::new(PacketType::Control, message_id, payload)
    }
    
    /// 创建心跳包
    pub fn heartbeat() -> Self {
        Self::new(PacketType::Heartbeat, 0, BytesMut::new())
    }
    
    /// 创建回显包
    pub fn echo(message_id: u32, payload: impl Into<BytesMut>) -> Self {
        Self::new(PacketType::Echo, message_id, payload)
    }
    
    /// 创建错误包
    pub fn error(message_id: u32, error_msg: &str) -> Self {
        Self::new(PacketType::Error, message_id, error_msg.as_bytes())
    }
    
    /// 序列化为字节
    pub fn to_bytes(&self) -> BytesMut {
        let mut buffer = BytesMut::with_capacity(9 + self.payload.len());
        
        // 写入包类型（1字节）
        buffer.extend_from_slice(&[self.packet_type.into()]);
        
        // 写入消息ID（4字节，大端序）
        buffer.extend_from_slice(&self.message_id.to_be_bytes());
        
        // 写入负载长度（4字节，大端序）
        buffer.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        
        // 写入负载
        buffer.extend_from_slice(&self.payload);
        
        buffer
    }
    
    /// 从字节反序列化
    pub fn from_bytes(data: &[u8]) -> Result<Self, PacketError> {
        if data.len() < 9 {
            return Err(PacketError::InvalidFormat("数据太短".to_string()));
        }
        
        // 读取包类型
        let packet_type = PacketType::from(data[0]);
        
        // 读取消息ID
        let message_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        
        // 读取负载长度
        let payload_len = u32::from_be_bytes([data[5], data[6], data[7], data[8]]) as usize;
        
        // 检查数据完整性
        if data.len() != 9 + payload_len {
            return Err(PacketError::InvalidFormat("数据长度不匹配".to_string()));
        }
        
        // 读取负载
        let payload = BytesMut::from(&data[9..]);
        
        Ok(Self {
            packet_type,
            message_id,
            payload,
        })
    }
    
    /// 获取负载的字符串表示（如果是有效UTF-8）
    pub fn payload_as_string(&self) -> Option<String> {
        String::from_utf8(self.payload.to_vec()).ok()
    }
    
    /// 检查是否为心跳包
    pub fn is_heartbeat(&self) -> bool {
        self.packet_type == PacketType::Heartbeat
    }
    
    /// 检查是否为控制包
    pub fn is_control(&self) -> bool {
        self.packet_type == PacketType::Control
    }
    
    /// 检查是否为数据包
    pub fn is_data(&self) -> bool {
        self.packet_type == PacketType::Data
    }
    
    /// 检查是否为错误包
    pub fn is_error(&self) -> bool {
        self.packet_type == PacketType::Error
    }
}

/// 数据包错误类型
#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error("Invalid packet format: {0}")]
    InvalidFormat(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Buffer overflow")]
    BufferOverflow,
}



#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_packet_serialization() {
        let packet = UnifiedPacket::data(123, "Hello, World!");
        let bytes = packet.to_bytes();
        let decoded = UnifiedPacket::from_bytes(&bytes).unwrap();
        
        assert_eq!(packet, decoded);
        assert_eq!(decoded.payload_as_string().unwrap(), "Hello, World!");
    }
    
    #[test]
    fn test_packet_types() {
        assert!(UnifiedPacket::heartbeat().is_heartbeat());
        assert!(UnifiedPacket::data(1, "test").is_data());
        assert!(UnifiedPacket::control(1, "test").is_control());
        assert!(UnifiedPacket::error(1, "error").is_error());
    }
} 