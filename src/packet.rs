use bytes::BytesMut;

#[derive(Debug, Clone)]
pub struct Packet {
    pub message_id: u32,
    pub payload: BytesMut,
}

impl Packet {
    pub fn new(message_id: u32, payload: Vec<u8>) -> Self {
        Packet {
            message_id,
            payload: BytesMut::from(&payload[..]),
        }
    }

    pub fn to_bytes(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(4 + self.payload.len());
        buf.extend_from_slice(&self.message_id.to_le_bytes());
        buf.extend_from_slice(&self.payload);
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let message_id = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let payload = BytesMut::from(&bytes[4..]);
        Packet { message_id, payload }
    }
}