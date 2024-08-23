# MsgTrans for rust

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
