use bytes::BytesMut;
use crate::compression::CompressionMethod;

#[derive(Debug, Clone)]
pub struct Packet {
    pub message_id: u32,
    pub message_length: u32,
    pub compression_type: CompressionMethod,
    pub extend_length: u32,
    pub extend_header: BytesMut,
    pub payload: BytesMut,
}

impl Packet {
    pub fn new(message_id: u32, payload: Vec<u8>) -> Self {
        let message_length = payload.len() as u32;
        Packet {
            message_id,
            message_length,
            compression_type: CompressionMethod::None,
            extend_length: 0,
            extend_header: BytesMut::new(),
            payload: BytesMut::from(&payload[..]),
        }
    }

    pub fn set_compression_type(mut self, compression_type: CompressionMethod) -> Self {
        self.compression_type = compression_type;
        self
    }

    pub fn get_compression_type(&self) -> &CompressionMethod {
        &self.compression_type
    }

    pub fn set_extend_header(mut self, extend_header: Vec<u8>) -> Self {
        self.extend_length = extend_header.len() as u32;
        self.extend_header = BytesMut::from(&extend_header[..]);
        self
    }

    pub fn get_extend_header(&self) -> &BytesMut {
        &self.extend_header
    }

    pub fn to_bytes(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(16 + self.extend_length as usize + self.payload.len());

        // 序列化头部内容
        buf.extend_from_slice(&self.message_id.to_le_bytes());
        buf.extend_from_slice(&self.message_length.to_le_bytes());
        buf.extend_from_slice(&(self.compression_type.clone() as u8).to_le_bytes());
        buf.extend_from_slice(&self.extend_length.to_le_bytes());

        // 序列化扩展头部内容
        buf.extend_from_slice(&self.extend_header);

        // 序列化payload内容
        buf.extend_from_slice(&self.payload);

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        // 解析头部内容
        let message_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let message_length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let compression_type = match bytes[8] {
            0 => CompressionMethod::None,
            1 => CompressionMethod::Zstd,
            2 => CompressionMethod::Zlib,
            _ => CompressionMethod::None, // 默认值
        };
        let extend_length = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]) as usize;

        // 解析扩展头部内容
        let extend_header = BytesMut::from(&bytes[13..13 + extend_length]);

        // 解析payload内容
        let payload = BytesMut::from(&bytes[13 + extend_length..]);

        Packet {
            message_id,
            message_length,
            compression_type,
            extend_length: extend_length as u32,
            extend_header,
            payload,
        }
    }
}
