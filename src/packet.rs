/// 统一架构数据包定义
/// 
/// 为统一架构设计的简化、高效的数据包格式

use std::sync::atomic::{AtomicU32, Ordering};

/// 数据包类型 - 简化为3种核心类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketType {
    /// 单向消息（不需要回复）
    OneWay = 0,
    /// 请求（需要回复）
    Request = 1,
    /// 回复消息
    Response = 2,
}

impl PacketType {
    /// 向后兼容：Data 类型别名
    pub const Data: PacketType = PacketType::OneWay;
}

impl From<u8> for PacketType {
    fn from(value: u8) -> Self {
        match value {
            0 => PacketType::OneWay,
            1 => PacketType::Request,
            2 => PacketType::Response,
            _ => PacketType::OneWay, // 默认值
        }
    }
}

impl From<PacketType> for u8 {
    fn from(packet_type: PacketType) -> Self {
        packet_type as u8
    }
}

/// 压缩类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompressionType {
    None = 0,
    Zstd = 1,
    Zlib = 2,
}

impl From<u8> for CompressionType {
    fn from(value: u8) -> Self {
        match value {
            0 => CompressionType::None,
            1 => CompressionType::Zstd,
            2 => CompressionType::Zlib,
            _ => CompressionType::None,
        }
    }
}

impl From<CompressionType> for u8 {
    fn from(compression: CompressionType) -> Self {
        compression as u8
    }
}

/// 保留字段标志位
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedFlags(u16);

impl ReservedFlags {
    /// 创建空标志
    pub fn new() -> Self {
        Self(0)
    }
    
    /// 设置分片标志
    pub fn with_fragmented(mut self, fragmented: bool) -> Self {
        if fragmented {
            self.0 |= 0x0001;
        } else {
            self.0 &= !0x0001;
        }
        self
    }
    
    /// 检查是否分片
    pub fn is_fragmented(&self) -> bool {
        (self.0 & 0x0001) != 0
    }
    
    /// 设置优先级标志
    pub fn with_priority(mut self, high_priority: bool) -> Self {
        if high_priority {
            self.0 |= 0x0002;
        } else {
            self.0 &= !0x0002;
        }
        self
    }
    
    /// 检查是否高优先级
    pub fn is_high_priority(&self) -> bool {
        (self.0 & 0x0002) != 0
    }
    
    /// 设置路由标签
    pub fn with_route_tag(mut self, has_route: bool) -> Self {
        if has_route {
            self.0 |= 0x0004;
        } else {
            self.0 &= !0x0004;
        }
        self
    }
    
    /// 检查是否有路由标签
    pub fn has_route_tag(&self) -> bool {
        (self.0 & 0x0004) != 0
    }
    
    /// 获取原始值
    pub fn raw(&self) -> u16 {
        self.0
    }
    
    /// 从原始值创建
    pub fn from_raw(value: u16) -> Self {
        Self(value)
    }
}

impl Default for ReservedFlags {
    fn default() -> Self {
        Self::new()
    }
}

/// 16字节固定头部 - 优化的字段顺序
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedHeader {
    /// 协议版本 (1字节)
    pub version: u8,
    /// 压缩算法 (1字节)
    pub compression: CompressionType,
    /// 数据包类型 (1字节)
    pub packet_type: PacketType,
    /// 应用层业务类型 (1字节) - 0-255，由业务层自定义
    pub biz_type: u8,
    /// 消息ID (4字节)
    pub message_id: u32,
    /// 扩展头长度 (2字节)
    pub ext_header_len: u16,
    /// 负载长度 (4字节)
    pub payload_len: u32,
    /// 保留字段 (2字节) - 分片、优先级、路由等标志
    pub reserved: ReservedFlags,
}

impl FixedHeader {
    /// 创建新的固定头部
    pub fn new(packet_type: PacketType, message_id: u32) -> Self {
        Self {
            version: 1,
            compression: CompressionType::None,
            packet_type,
            biz_type: 0, // 默认业务类型
            message_id,
            ext_header_len: 0,
            payload_len: 0,
            reserved: ReservedFlags::new(),
        }
    }
    
    /// 序列化为字节数组 (大端序)
    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0] = self.version;
        bytes[1] = u8::from(self.compression);
        bytes[2] = u8::from(self.packet_type);
        bytes[3] = self.biz_type;
        bytes[4..8].copy_from_slice(&self.message_id.to_be_bytes());
        bytes[8..10].copy_from_slice(&self.ext_header_len.to_be_bytes());
        bytes[10..14].copy_from_slice(&self.payload_len.to_be_bytes());
        bytes[14..16].copy_from_slice(&self.reserved.raw().to_be_bytes());
        bytes
    }
    
    /// 从字节数组反序列化 (大端序)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PacketError> {
        if bytes.len() < 16 {
            return Err(PacketError::InvalidHeader("Header too short".to_string()));
        }
        
        let version = bytes[0];
        if version != 1 {
            return Err(PacketError::UnsupportedVersion(version));
        }
        
        let compression = CompressionType::from(bytes[1]);
        let packet_type = PacketType::from(bytes[2]);
        let biz_type = bytes[3];
        
        let message_id = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let ext_header_len = u16::from_be_bytes([bytes[8], bytes[9]]);
        let payload_len = u32::from_be_bytes([bytes[10], bytes[11], bytes[12], bytes[13]]);
        let reserved = ReservedFlags::from_raw(u16::from_be_bytes([bytes[14], bytes[15]]));
        
        Ok(Self {
            version,
            compression,
            packet_type,
            biz_type,
            message_id,
            ext_header_len,
            payload_len,
            reserved,
        })
    }
}

/// 消息ID管理器 - 线程安全
#[derive(Debug)]
pub struct MessageIdManager {
    counter: AtomicU32,
}

impl MessageIdManager {
    /// 创建新的ID管理器
    pub fn new() -> Self {
        Self {
            counter: AtomicU32::new(1), // 从1开始
        }
    }
    
    /// 获取下一个ID
    pub fn next_id(&self) -> u32 {
        let id = self.counter.fetch_add(1, Ordering::SeqCst);
        if id == u32::MAX {
            // 达到最大值，重置为1
            self.counter.store(1, Ordering::SeqCst);
            1
        } else {
            id
        }
    }
    
    /// 重置ID计数器（用于连接重建）
    pub fn reset(&self) {
        self.counter.store(1, Ordering::SeqCst);
    }
    
    /// 获取当前ID（不递增）
    pub fn current_id(&self) -> u32 {
        self.counter.load(Ordering::SeqCst)
    }
}

impl Default for MessageIdManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 数据包结构
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    /// 固定头部
    pub header: FixedHeader,
    /// 扩展头部（可选）
    pub ext_header: Vec<u8>,
    /// 负载数据
    pub payload: Vec<u8>,
}

impl Packet {
    /// 创建新的数据包
    pub fn new(packet_type: PacketType, message_id: u32) -> Self {
        Self {
            header: FixedHeader::new(packet_type, message_id),
            ext_header: Vec::new(),
            payload: Vec::new(),
        }
    }
    
    /// 创建单向消息
    pub fn one_way(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        let mut packet = Self::new(PacketType::OneWay, message_id);
        packet.set_payload(payload);
        packet
    }
    
    /// 创建请求消息
    pub fn request(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        let mut packet = Self::new(PacketType::Request, message_id);
        packet.set_payload(payload);
        packet
    }
    
    /// 创建回复消息
    pub fn response(message_id: u32, payload: impl Into<Vec<u8>>) -> Self {
        let mut packet = Self::new(PacketType::Response, message_id);
        packet.set_payload(payload);
        packet
    }
    
    /// 设置负载
    pub fn set_payload(&mut self, payload: impl Into<Vec<u8>>) {
        self.payload = payload.into();
        self.header.payload_len = self.payload.len() as u32;
    }
    
    /// 设置消息ID
    pub fn set_message_id(&mut self, message_id: u32) {
        self.header.message_id = message_id;
    }
    
    /// 设置数据包类型
    pub fn set_packet_type(&mut self, packet_type: PacketType) {
        self.header.packet_type = packet_type;
    }
    
    /// 设置扩展头
    pub fn set_ext_header(&mut self, ext_header: impl Into<Vec<u8>>) {
        self.ext_header = ext_header.into();
        self.header.ext_header_len = self.ext_header.len() as u16;
    }
    
    /// 设置压缩类型
    pub fn set_compression(&mut self, compression: CompressionType) {
        self.header.compression = compression;
    }
    
    /// 设置分片标志
    pub fn set_fragmented(&mut self, fragmented: bool) {
        self.header.reserved = self.header.reserved.with_fragmented(fragmented);
    }
    
    /// 设置优先级
    pub fn set_priority(&mut self, high_priority: bool) {
        self.header.reserved = self.header.reserved.with_priority(high_priority);
    }
    
    /// 设置业务类型
    pub fn set_biz_type(&mut self, biz_type: u8) {
        self.header.biz_type = biz_type;
    }
    
    /// 获取业务类型
    pub fn biz_type(&self) -> u8 {
        self.header.biz_type
    }
    
    /// 获取压缩类型
    pub fn compression(&self) -> CompressionType {
        self.header.compression
    }
    
    /// 检查是否分片
    pub fn is_fragmented(&self) -> bool {
        self.header.reserved.is_fragmented()
    }
    
    /// 检查是否高优先级
    pub fn is_high_priority(&self) -> bool {
        self.header.reserved.is_high_priority()
    }
    
    /// 设置路由标签
    pub fn set_route_tag(&mut self, has_route: bool) {
        self.header.reserved = self.header.reserved.with_route_tag(has_route);
    }
    
    /// 检查是否有路由标签
    pub fn has_route_tag(&self) -> bool {
        self.header.reserved.has_route_tag()
    }
    
    /// 压缩负载
    pub fn compress_payload(&mut self) -> Result<(), PacketError> {
        let compression = self.header.compression;
        if compression == CompressionType::None {
            return Ok(());
        }
        
        self.payload = Self::compress_data(&self.payload, compression)?;
        self.header.payload_len = self.payload.len() as u32;
        Ok(())
    }
    
    /// 解压负载
    pub fn decompress_payload(&mut self) -> Result<(), PacketError> {
        let compression = self.header.compression;
        if compression == CompressionType::None {
            return Ok(());
        }
        
        self.payload = Self::decompress_data(&self.payload, compression)?;
        self.header.payload_len = self.payload.len() as u32;
        Ok(())
    }
    
    /// 序列化为字节数组
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        
        // 固定头部
        bytes.extend_from_slice(&self.header.to_bytes());
        
        // 扩展头部
        if !self.ext_header.is_empty() {
            bytes.extend_from_slice(&self.ext_header);
        }
        
        // 负载
        bytes.extend_from_slice(&self.payload);
        
        bytes
    }
    
    /// 从字节数组反序列化
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PacketError> {
        if bytes.len() < 16 {
            return Err(PacketError::InvalidPacket("Packet too short".to_string()));
        }
        
        // 解析固定头部
        let header = FixedHeader::from_bytes(&bytes[0..16])?;
        
        let mut offset = 16;
        
        // 解析扩展头部
        let ext_header = if header.ext_header_len > 0 {
            let end = offset + header.ext_header_len as usize;
            if bytes.len() < end {
                return Err(PacketError::InvalidPacket("Extended header incomplete".to_string()));
            }
            let ext_header = bytes[offset..end].to_vec();
            offset = end;
            ext_header
        } else {
            Vec::new()
        };
        
        // 解析负载
        let payload = if header.payload_len > 0 {
            let end = offset + header.payload_len as usize;
            if bytes.len() < end {
                return Err(PacketError::InvalidPacket("Payload incomplete".to_string()));
            }
            bytes[offset..end].to_vec()
        } else {
            Vec::new()
        };
        
        Ok(Self {
            header,
            ext_header,
            payload,
        })
    }
    
    /// 获取数据包类型
    pub fn packet_type(&self) -> PacketType {
        self.header.packet_type
    }
    
    /// 获取消息ID
    pub fn message_id(&self) -> u32 {
        self.header.message_id
    }
    
    /// 获取负载大小
    pub fn payload_len(&self) -> usize {
        self.payload.len()
    }
    
    /// 获取总大小
    pub fn total_len(&self) -> usize {
        16 + self.ext_header.len() + self.payload.len()
    }
    
    /// 获取负载的字符串表示（如果是有效UTF-8）
    pub fn payload_as_string(&self) -> Option<String> {
        String::from_utf8(self.payload.clone()).ok()
    }
    
    /// 压缩数据
    fn compress_data(data: &[u8], compression: CompressionType) -> Result<Vec<u8>, PacketError> {
        match compression {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Zlib => {
                #[cfg(feature = "flate2")]
                {
                    use flate2::{Compression, write::ZlibEncoder};
                    use std::io::Write;
                    
                    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
                    encoder.write_all(data).map_err(|e| PacketError::CompressionError(e.to_string()))?;
                    encoder.finish().map_err(|e| PacketError::CompressionError(e.to_string()))
                }
                #[cfg(not(feature = "flate2"))]
                Err(PacketError::UnsupportedCompression("flate2 feature not enabled".to_string()))
            }
            CompressionType::Zstd => {
                #[cfg(feature = "zstd")]
                {
                    zstd::bulk::compress(data, 3).map_err(|e| PacketError::CompressionError(e.to_string()))
                }
                #[cfg(not(feature = "zstd"))]
                Err(PacketError::UnsupportedCompression("zstd feature not enabled".to_string()))
            }
        }
    }
    
    /// 解压数据
    fn decompress_data(data: &[u8], compression: CompressionType) -> Result<Vec<u8>, PacketError> {
        match compression {
            CompressionType::None => Ok(data.to_vec()),
            CompressionType::Zlib => {
                #[cfg(feature = "flate2")]
                {
                    use flate2::read::ZlibDecoder;
                    use std::io::Read;
                    
                    let mut decoder = ZlibDecoder::new(data);
                    let mut result = Vec::new();
                    decoder.read_to_end(&mut result).map_err(|e| PacketError::CompressionError(e.to_string()))?;
                    Ok(result)
                }
                #[cfg(not(feature = "flate2"))]
                Err(PacketError::UnsupportedCompression("flate2 feature not enabled".to_string()))
            }
            CompressionType::Zstd => {
                #[cfg(feature = "zstd")]
                {
                    zstd::bulk::decompress(data, 1024 * 1024).map_err(|e| PacketError::CompressionError(e.to_string()))
                }
                #[cfg(not(feature = "zstd"))]
                Err(PacketError::UnsupportedCompression("zstd feature not enabled".to_string()))
            }
        }
    }
}

/// 数据包错误类型
#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error("Invalid header: {0}")]
    InvalidHeader(String),
    
    #[error("Invalid packet: {0}")]
    InvalidPacket(String),
    
    #[error("Unsupported version: {0}")]
    UnsupportedVersion(u8),
    
    #[error("Compression error: {0}")]
    CompressionError(String),
    
    #[error("Unsupported compression: {0}")]
    UnsupportedCompression(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_type_conversion() {
        assert_eq!(u8::from(PacketType::OneWay), 0);
        assert_eq!(u8::from(PacketType::Request), 1);
        assert_eq!(u8::from(PacketType::Response), 2);
        
        assert_eq!(PacketType::from(0), PacketType::OneWay);
        assert_eq!(PacketType::from(1), PacketType::Request);
        assert_eq!(PacketType::from(2), PacketType::Response);
    }

    #[test]
    fn test_compression_type_conversion() {
        assert_eq!(u8::from(CompressionType::None), 0);
        assert_eq!(u8::from(CompressionType::Zstd), 1);
        assert_eq!(u8::from(CompressionType::Zlib), 2);
        
        assert_eq!(CompressionType::from(0), CompressionType::None);
        assert_eq!(CompressionType::from(1), CompressionType::Zstd);
        assert_eq!(CompressionType::from(2), CompressionType::Zlib);
    }

    #[test]
    fn test_fixed_header_serialization() {
        let header = FixedHeader {
            version: 1,
            compression: CompressionType::Zstd,
            packet_type: PacketType::Request,
            biz_type: 0,
            message_id: 12345,
            ext_header_len: 8,
            payload_len: 1024,
            reserved: ReservedFlags::new(),
        };
        
        let bytes = header.to_bytes();
        let recovered = FixedHeader::from_bytes(&bytes).unwrap();
        
        assert_eq!(header, recovered);
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn test_message_id_manager() {
        let manager = MessageIdManager::new();
        
        assert_eq!(manager.next_id(), 1);
        assert_eq!(manager.next_id(), 2);
        assert_eq!(manager.next_id(), 3);
        
        manager.reset();
        assert_eq!(manager.next_id(), 1);
    }

    #[test]
    fn test_packet_creation() {
        let mut packet = Packet::one_way(123, b"hello world");
        packet.set_compression(CompressionType::Zstd);
        packet.set_fragmented(true);
        
        assert_eq!(packet.header.packet_type, PacketType::OneWay);
        assert_eq!(packet.header.message_id, 123);
        assert_eq!(packet.payload_len(), 11);
        assert_eq!(packet.header.compression, CompressionType::Zstd);
        assert!(packet.header.reserved.is_fragmented());
    }

    #[test]
    fn test_packet_serialization() {
        let packet = Packet::request(456, "test message");
        let bytes = packet.to_bytes();
        let recovered = Packet::from_bytes(&bytes).unwrap();
        
        assert_eq!(packet, recovered);
    }

    #[test]
    fn test_packet_with_ext_header() {
        let mut packet = Packet::response(789, "response data");
        packet.set_ext_header(b"extension");
        
        let bytes = packet.to_bytes();
        let recovered = Packet::from_bytes(&bytes).unwrap();
        
        assert_eq!(packet, recovered);
        assert_eq!(recovered.ext_header, b"extension");
    }

    #[test]
    fn test_compression() {
        let original_data = b"Hello, World! This is a test message for compression.".repeat(10);
        
        // 测试无压缩
        let compressed = Packet::compress_data(&original_data, CompressionType::None).unwrap();
        let decompressed = Packet::decompress_data(&compressed, CompressionType::None).unwrap();
        assert_eq!(original_data, decompressed);
        
        // 测试 Zlib 压缩（如果启用）
        #[cfg(feature = "flate2")]
        {
            let compressed = Packet::compress_data(&original_data, CompressionType::Zlib).unwrap();
            let decompressed = Packet::decompress_data(&compressed, CompressionType::Zlib).unwrap();
            assert_eq!(original_data, decompressed);
        }
        
        // 测试 Zstd 压缩（如果启用）
        #[cfg(feature = "zstd")]
        {
            let compressed = Packet::compress_data(&original_data, CompressionType::Zstd).unwrap();
            let decompressed = Packet::decompress_data(&compressed, CompressionType::Zstd).unwrap();
            assert_eq!(original_data, decompressed);
        }
    }

    #[test]
    fn test_reserved_flags() {
        let mut flags = ReservedFlags::new();
        assert!(!flags.is_fragmented());
        assert!(!flags.is_high_priority());
        assert!(!flags.has_route_tag());
        
        flags = flags.with_fragmented(true);
        assert!(flags.is_fragmented());
        
        flags = flags.with_priority(true);
        assert!(flags.is_high_priority());
        
        flags = flags.with_route_tag(true);
        assert!(flags.has_route_tag());
    }

    #[test]
    fn test_packet_creation_with_new_fields() {
        let mut packet = Packet::one_way(123, b"hello world");
        packet.set_compression(CompressionType::Zstd);
        packet.set_biz_type(42); // 业务层自定义类型
        packet.set_fragmented(true);
        packet.set_priority(true);
        packet.set_route_tag(true);
        
        assert_eq!(packet.header.packet_type, PacketType::OneWay);
        assert_eq!(packet.header.message_id, 123);
        assert_eq!(packet.payload_len(), 11);
        assert_eq!(packet.header.compression, CompressionType::Zstd);
        assert_eq!(packet.header.biz_type, 42);
        assert!(packet.header.reserved.is_fragmented());
        assert!(packet.header.reserved.is_high_priority());
        assert!(packet.header.reserved.has_route_tag());
    }

    #[test]
    fn test_packet_serialization_with_new_format() {
        let mut packet = Packet::request(456, "test message");
        packet.set_biz_type(123); // 业务层自定义类型
        packet.set_compression(CompressionType::Zlib);
        
        let bytes = packet.to_bytes();
        let recovered = Packet::from_bytes(&bytes).unwrap();
        
        assert_eq!(packet, recovered);
        assert_eq!(recovered.biz_type(), 123);
        assert_eq!(recovered.compression(), CompressionType::Zlib);
    }

    #[test]
    fn test_new_byte_order_format() {
        let mut packet = Packet::request(0x12345678, "test");
        packet.set_biz_type(255); // 最大业务类型值
        packet.set_compression(CompressionType::Zstd);
        
        let bytes = packet.to_bytes();
        
        // 验证新的字段顺序
        assert_eq!(bytes[0], 1); // version
        assert_eq!(bytes[1], 1); // compression = Zstd
        assert_eq!(bytes[2], 1); // packet_type = Request
        assert_eq!(bytes[3], 255); // biz_type = 255
        
        // message_id 在字节 4-7 位置，大端序
        assert_eq!(bytes[4], 0x12);
        assert_eq!(bytes[5], 0x34);
        assert_eq!(bytes[6], 0x56);
        assert_eq!(bytes[7], 0x78);
        
        // ext_header_len 在字节 8-9
        assert_eq!(bytes[8], 0x00);
        assert_eq!(bytes[9], 0x00);
        
        // payload_len 在字节 10-13
        assert_eq!(bytes[10], 0x00);
        assert_eq!(bytes[11], 0x00);
        assert_eq!(bytes[12], 0x00);
        assert_eq!(bytes[13], 0x04); // "test" = 4 bytes
    }

    #[test]
    fn test_big_endian_format() {
        let packet = Packet::request(0x12345678, "test");
        let bytes = packet.to_bytes();
        
        // 验证 big endian 格式
        // message_id 在新字段顺序中应该在字节 4-7 位置，大端序
        assert_eq!(bytes[4], 0x12);
        assert_eq!(bytes[5], 0x34);
        assert_eq!(bytes[6], 0x56);
        assert_eq!(bytes[7], 0x78);
    }
} 