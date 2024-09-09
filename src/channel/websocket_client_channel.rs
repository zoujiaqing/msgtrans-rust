use anyhow::Result;
use super::ClientChannel;
use crate::packet::{Packet, PacketHeader};
use crate::callbacks::{
    OnReconnectHandler, OnClientDisconnectHandler, OnClientErrorHandler, OnSendHandler, OnClientMessageHandler,
};
use tokio::net::{TcpStream, TcpSocket};
use tokio_tungstenite::{WebSocketStream, client_async};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::MaybeTlsStream;
use futures::stream::StreamExt;
use futures::sink::SinkExt;
use url::Url;
use tokio::sync::Mutex;
use std::net::{SocketAddr, IpAddr};
use std::sync::Arc;
use std::io;

pub struct WebSocketClientChannel {
    send_stream: Option<Arc<Mutex<futures::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>>>>,
    receive_stream: Option<Arc<Mutex<futures::stream::SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>>,
    address: String,
    port: u16,
    path: String,
    reconnect_handler: Option<Arc<Mutex<OnReconnectHandler>>>,
    disconnect_handler: Option<Arc<Mutex<OnClientDisconnectHandler>>>,
    error_handler: Option<Arc<Mutex<OnClientErrorHandler>>>,
    send_handler: Option<Arc<Mutex<OnSendHandler>>>,
    message_handler: Option<Arc<Mutex<OnClientMessageHandler>>>,
    local_ip: Option<String>,
}

impl WebSocketClientChannel {
    pub fn new(address: &str, port: u16, path: &str) -> Self {
        WebSocketClientChannel {
            send_stream: None,
            receive_stream: None,
            address: address.to_string(),
            port,
            path: path.to_string(),
            reconnect_handler: None,
            disconnect_handler: None,
            error_handler: None,
            send_handler: None,
            message_handler: None,
            local_ip: None,
        }
    }
}

#[async_trait::async_trait]
impl ClientChannel for WebSocketClientChannel {

    fn bind_local_addr(&mut self, ip_addr: String) {
        self.local_ip = Some(ip_addr);
    }

    async fn connect(&mut self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let url = Url::parse(&format!("ws://{}:{}{}", self.address, self.port, self.path))?;
    
        let tcp_stream = match &self.local_ip {
            Some(local_ip) => {
                let local_addr: IpAddr = local_ip.parse()?;
                let local_socket = SocketAddr::new(local_addr, 0);
                let socket = TcpSocket::new_v4()?;
                socket.bind(local_socket)?;
                socket.connect(SocketAddr::new(self.address.parse()?, self.port)).await?
            }
            None => TcpStream::connect(format!("{}:{}", self.address, self.port)).await?,
        };
    
        // Wrap the TcpStream in MaybeTlsStream
        let stream = MaybeTlsStream::Plain(tcp_stream);
        let ws_stream = client_async(url, stream).await?;
    
        let (send_stream, receive_stream) = ws_stream.0.split();
        self.send_stream = Some(Arc::new(Mutex::new(send_stream)));
        self.receive_stream = Some(Arc::new(Mutex::new(receive_stream)));
    
        if let Some(ref handler) = self.reconnect_handler {
            let handler = handler.lock().await;
            handler();
        }
    
        let receive_stream = self.receive_stream.clone();
        let message_handler = self.message_handler.clone();
        let disconnect_handler = self.disconnect_handler.clone();
        let error_handler = self.error_handler.clone();
    
        tokio::spawn(async move {
            if let Some(receive_stream) = receive_stream {
                let mut receive_stream = receive_stream.lock().await;
                while let Some(msg) = receive_stream.next().await {
                    match msg {
                        Ok(Message::Binary(bin)) => {
                            if let Some(ref handler_arc) = message_handler {
                                let handler = handler_arc.lock().await;
                                
                                if bin.len() < 16 {
                                    println!("Msg length error, length: {}", bin.len());
                                    break;
                                }
    
                                // Check if we have enough data to parse the PacketHeader
                                let header = PacketHeader::from_bytes(&bin[..16]);
        
                                // Check if the full packet is available
                                let total_length = 16 + header.extend_length as usize + header.message_length as usize;
                                if bin.len() < total_length {
                                    println!("Not enough data, length: {}", bin.len());
                                    break;
                                }
        
                                // Parse the full packet
                                let packet = Packet::by_header_from_bytes(header, &bin[16..total_length]);
                                (*handler)(packet);
                            }
                        }
                        Ok(_) => continue,
                        Err(e) => {
                            if let Some(handler_arc) = &error_handler {
                                let handler = handler_arc.lock().await;
                                (handler)(Box::new(e));
                            }
                            break;
                        }
                    }
                }
    
                if let Some(handler_arc) = &disconnect_handler {
                    let handler = handler_arc.lock().await;
                    (handler)();
                }
            }
        });
    
        Ok(())
    }

    async fn send(
        &mut self,
        packet: Packet,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if let Some(send_stream) = &self.send_stream {
            let mut send_stream = send_stream.lock().await;
            let send_result = send_stream
                .send(Message::Binary(packet.to_bytes().to_vec()))
                .await;

            if let Some(ref handler) = self.send_handler {
                let handler = handler.lock().await;
                let send_result_for_handler = send_result
                    .as_ref()
                    .map(|_| ())
                    .map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>);
                handler(packet.clone(), send_result_for_handler);
            }

            send_result.map(|_| ()).map_err(|e| Box::new(io::Error::new(io::ErrorKind::Other, e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
        } else {
            let err_msg: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established"));

            if let Some(ref handler) = self.error_handler {
                let handler = handler.lock().await;
                handler(err_msg);
            }

            Err(Box::new(io::Error::new(io::ErrorKind::NotConnected, "No connection established")))
        }
    }

    fn set_reconnect_handler(&mut self, handler: Arc<Mutex<OnReconnectHandler>>) {
        self.reconnect_handler = Some(handler);
    }

    fn set_disconnect_handler(&mut self, handler: Arc<Mutex<OnClientDisconnectHandler>>) {
        self.disconnect_handler = Some(handler);
    }

    fn set_error_handler(&mut self, handler: Arc<Mutex<OnClientErrorHandler>>) {
        self.error_handler = Some(handler);
    }

    fn set_send_handler(&mut self, handler: Arc<Mutex<OnSendHandler>>) {
        self.send_handler = Some(handler);
    }

    fn set_message_handler(&mut self, handler: Arc<Mutex<OnClientMessageHandler>>) {
        self.message_handler = Some(handler);
    }
}