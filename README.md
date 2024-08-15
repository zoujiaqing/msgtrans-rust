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
