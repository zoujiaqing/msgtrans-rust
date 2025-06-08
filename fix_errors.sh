#!/bin/bash

# 修复Connection错误
sed -i '' 's/TransportError::Connection(\([^)]*\))/TransportError::connection_error(\1, true)/g' src/adapters/*.rs
sed -i '' 's/TransportError::Connection { reason:/TransportError::connection_error(/g' src/adapters/*.rs
sed -i '' 's/, retryable: [^}]* }/true)/g' src/adapters/*.rs

# 修复Protocol错误
sed -i '' 's/TransportError::Protocol(\([^)]*\))/TransportError::protocol_error("generic", \1)/g' src/adapters/*.rs src/event.rs src/connection.rs

# 修复Configuration错误
sed -i '' 's/TransportError::Configuration(\([^)]*\))/TransportError::config_error("general", \1)/g' src/adapters/*.rs

# 修复ProtocolConfiguration错误
sed -i '' 's/TransportError::ProtocolConfiguration(\([^)]*\))/TransportError::config_error("protocol", \1)/g' src/adapters/*.rs

# 修复特殊错误类型
sed -i '' 's/TransportError::Io(\([^)]*\))/TransportError::connection_error(format!("IO error: {:?}", \1), true)/g' src/adapters/*.rs src/event.rs
sed -i '' 's/TransportError::Authentication(\([^)]*\))/TransportError::protocol_error("auth", \1)/g' src/adapters/*.rs
sed -i '' 's/TransportError::Serialization(\([^)]*\))/TransportError::protocol_error("serialization", \1)/g' src/adapters/*.rs
sed -i '' 's/TransportError::Plugin(\([^)]*\))/TransportError::config_error("plugin", \1)/g' src/plugin.rs

echo "错误修复完成" 