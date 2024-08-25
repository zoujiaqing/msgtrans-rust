use bytes::BytesMut;
use crate::compression::CompressionMethod;

#[derive(Debug, Clone)]
pub struct PacketHeader {
    pub message_id: u32,
    pub message_length: u32,
    pub compression_type: CompressionMethod,
    pub extend_length: u32,
}

impl PacketHeader {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let message_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let message_length = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let compression_type = match bytes[8] {
            0 => CompressionMethod::None,
            1 => CompressionMethod::Zstd,
            2 => CompressionMethod::Zlib,
            _ => CompressionMethod::None, // Default value
        };
        let extend_length = u32::from_le_bytes([bytes[9], bytes[10], bytes[11], bytes[12]]);

        PacketHeader {
            message_id,
            message_length,
            compression_type,
            extend_length,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub header: PacketHeader,
    pub extend_header: BytesMut,
    pub payload: BytesMut,
}

impl Packet {
    pub fn new(header: PacketHeader, extend_header: Vec<u8>, payload: Vec<u8>) -> Self {
        Packet {
            header,
            extend_header: BytesMut::from(&extend_header[..]),
            payload: BytesMut::from(&payload[..]),
        }
    }

    pub fn from_bytes(header: PacketHeader, bytes: &[u8]) -> Self {
        let extend_length = header.extend_length as usize;
        let extend_header = BytesMut::from(&bytes[..extend_length]);
        let payload = BytesMut::from(&bytes[extend_length..]);

        Packet {
            header,
            extend_header,
            payload,
        }
    }

    pub fn to_bytes(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(
            16 + self.header.extend_length as usize + self.header.message_length as usize,
        );

        // Serialize the header content
        buf.extend_from_slice(&self.header.message_id.to_le_bytes());
        buf.extend_from_slice(&self.header.message_length.to_le_bytes());
        buf.extend_from_slice(&(self.header.compression_type.clone() as u8).to_le_bytes());
        buf.extend_from_slice(&self.header.extend_length.to_le_bytes());

        // Reserved
        buf.extend_from_slice(&(0 as u8).to_le_bytes());
        buf.extend_from_slice(&(0 as u8).to_le_bytes());
        buf.extend_from_slice(&(0 as u8).to_le_bytes());

        // Serialize the extended header content
        buf.extend_from_slice(&self.extend_header);

        // Serialize the payload content
        buf.extend_from_slice(&self.payload);

        buf
    }
}