# MsgTrans for rust

## Server sample code

Using MsgTrans to create multiple protocol server.

```rust
let mut server = MessageTransportServer::new();

// Add tcp channel
server.add_channel(Arc::new(Mutex::new(TcpServerChannel::new("0.0.0.0", 9001)))).await;

// Add WebSocket channel
server.add_channel(Arc::new(Mutex::new(WebSocketServerChannel::new("0.0.0.0", 9002, "/ws")))).await;

// Add QUIC channel
server.add_channel(Arc::new(Mutex::new(QuicServerChannel::new(
    "0.0.0.0",
    9003,
    "certs/cert.pem",
    "certs/key.pem",
)))).await;

// set some callback handler for server

server.start().await;
```

## Run example for server

```bash
cargo run --example server
```

## Run example for client

```bash
# for tcp
cargo run --example client_tcp
# for websocket
cargo run --example client_websocket
# for quic
cargo run --example client_quic
```

## Generate cert and key for test

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365000 -nodes -subj "/CN=localhost"
```

## Place needs improving

 1. The callback method can be set after startup.
 2. Processing after disconnection.
 3. Stability enhancement.
 4. Performance test.
 5. Add worker queue.
 6. TCP and QUIC sticky packet and unpacket.

## Packet Structure

```text
+--------------------------------+
|         Header Content         |
|  +--------------------------+  |
|  |  Message ID (4 bytes)    |  |
|  +--------------------------+  |
|  |  Message Length (4 bytes)|  |
|  +--------------------------+  |
|  | Compression Type (1 byte)|  |
|  +--------------------------+  |
|  |  Extend Length (4 bytes) |  |
|  +--------------------------+  |
+--------------------------------+
              |
              v
+--------------------------------+
|    Extended Header Content     |
|  (variable length, Extend      |
|   Length specifies size)       |
+--------------------------------+
              |
              v
+--------------------------------+
|        Payload Content         |
|    (variable length, Message   |
|     Length specifies size)     |
+--------------------------------+
```

Structure Explanation:

 1. Header Content: Contains fixed-length header information, including Message ID, Message Length, Compression Type, and Extend Length.
 2. Extended Header Content: Variable-length extended header content, with its size specified by the Extend Length field.
 3. Payload Content: Variable-length payload content, with its size specified by the Message Length field.
