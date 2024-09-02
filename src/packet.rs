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
            _ => CompressionMethod::None, // Default value.
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

    // Create Packet from bytes based on a given header.
    pub fn by_header_from_bytes(header: PacketHeader, bytes: &[u8]) -> Self {
        let extend_length = header.extend_length as usize;
        let extend_header = BytesMut::from(&bytes[..extend_length]);
        let payload = BytesMut::from(&bytes[extend_length..]);

        // Decompress extend_header.
        let decompressed_extend_header = match header.compression_type.decode(&extend_header) {
            Ok(decompressed) => decompressed,
            Err(e) => {
                eprintln!("Failed to decompress extend header: {}", e);
                extend_header.to_vec() // Use original extend_header if decompression fails.
            }
        };

        // Decompress payload.
        let decompressed_payload = match header.compression_type.decode(&payload) {
            Ok(decompressed) => decompressed,
            Err(e) => {
                eprintln!("Failed to decompress payload: {}", e);
                payload.to_vec() // Use original payload if decompression fails.
            }
        };

        let mut header_clone = header.clone();
        header_clone.extend_length = decompressed_extend_header.len() as u32;
        header_clone.message_length = decompressed_payload.len() as u32;

        Packet {
            header: header_clone,
            extend_header: BytesMut::from(&decompressed_extend_header[..]),
            payload: BytesMut::from(&decompressed_payload[..]),
        }
    }

    // Create Packet from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let header = PacketHeader::from_bytes(&bytes[..16]);
        Self::by_header_from_bytes(header, &bytes[16..])
    }

    // Convert Packet to bytes.
    pub fn to_bytes(&self) -> BytesMut {
        // Compress extend_header.
        let compressed_extend_header = match self.header.compression_type.encode(&self.extend_header) {
            Ok(compressed) => compressed,
            Err(e) => {
                eprintln!("Failed to compress extend header: {}", e);
                self.extend_header.to_vec() // Use original extend_header if compression fails.
            }
        };

        // Compress payload.
        let compressed_payload = match self.header.compression_type.encode(&self.payload) {
            Ok(compressed) => compressed,
            Err(e) => {
                eprintln!("Failed to compress payload: {}", e);
                self.payload.to_vec() // Use original payload if compression fails.
            }
        };

        // Calculate compressed lengths and update header.
        let mut header = self.header.clone();
        header.extend_length = compressed_extend_header.len() as u32;
        header.message_length = compressed_payload.len() as u32;

        let mut buf = BytesMut::with_capacity(
            16 + header.extend_length as usize + header.message_length as usize,
        );

        // Serialize the header content.
        buf.extend_from_slice(&header.message_id.to_le_bytes());
        buf.extend_from_slice(&header.message_length.to_le_bytes());
        buf.extend_from_slice(&(header.compression_type.clone() as u8).to_le_bytes());
        buf.extend_from_slice(&header.extend_length.to_le_bytes());

        // Reserved.
        buf.extend_from_slice(&(0 as u8).to_le_bytes());
        buf.extend_from_slice(&(0 as u8).to_le_bytes());
        buf.extend_from_slice(&(0 as u8).to_le_bytes());

        // Serialize the compressed extend_header content.
        buf.extend_from_slice(&compressed_extend_header);

        // Serialize the compressed payload content.
        buf.extend_from_slice(&compressed_payload);

        buf
    }
}
