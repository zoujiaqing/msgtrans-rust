use super::ClientChannel;
use crate::packet::Packet;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use bytes::BytesMut;

pub struct TcpClientChannel {
    stream: Option<TcpStream>,
    address: String,
    port: u16,
}

impl TcpClientChannel {
    pub fn new(address: &str, port: u16) -> Self {
        TcpClientChannel {
            stream: None,
            address: address.to_string(),
            port,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for TcpClientChannel {
    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stream = TcpStream::connect((&self.address[..], self.port)).await?;
        self.stream = Some(stream);
        Ok(())
    }

    async fn send(&mut self, packet: Packet) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            let data = packet.to_bytes();
            stream.write_all(&data).await?;
        } else {
            return Err("No connection established".into());
        }
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Packet>, Box<dyn std::error::Error + Send + Sync>> {
        if let Some(stream) = &mut self.stream {
            let mut buf = BytesMut::with_capacity(1024);
            let n = stream.read_buf(&mut buf).await?;
            if n > 0 {
                return Ok(Some(Packet::from_bytes(&buf[..n])));
            }
        } else {
            return Err("No connection established".into());
        }
        Ok(None)
    }
}