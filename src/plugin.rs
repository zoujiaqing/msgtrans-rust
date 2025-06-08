use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use async_trait::async_trait;
use crate::{
    error::TransportError,
    connection::{Connection, Server, ConnectionFactory},
    protocol::adapter::ConfigError,
};

/// 协议插件接口
#[async_trait]
pub trait ProtocolPlugin: Send + Sync {
    /// 获取协议名称
    fn name(&self) -> &str;
    
    /// 获取协议描述
    fn description(&self) -> &str;
    
    /// 获取协议特性
    fn features(&self) -> Vec<String>;
    
    /// 获取默认端口
    fn default_port(&self) -> Option<u16>;
    
    /// 验证配置
    async fn validate_config(&self, config: &dyn std::any::Any) -> Result<(), ConfigError>;
    
    /// 创建服务器
    async fn create_server(&self, config: Box<dyn std::any::Any>) -> Result<Box<dyn Server<Connection = Box<dyn Connection>>>, TransportError>;
    
    /// 创建连接工厂
    async fn create_connection_factory(&self, config: Box<dyn std::any::Any>) -> Result<Box<dyn ConnectionFactory<Connection = Box<dyn Connection>>>, TransportError>;
    
    /// 支持的配置类型名称
    fn config_type_name(&self) -> &str;
}

/// 插件注册表
pub struct PluginRegistry {
    plugins: Arc<RwLock<HashMap<String, Arc<dyn ProtocolPlugin>>>>,
}

impl PluginRegistry {
    /// 创建新的插件注册表
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 注册插件
    pub fn register(&self, plugin: Arc<dyn ProtocolPlugin>) -> Result<(), TransportError> {
        let mut plugins = self.plugins.write().unwrap();
        let name = plugin.name().to_string();
        
        if plugins.contains_key(&name) {
            return Err(TransportError::config_error("plugin", format!(
                "Protocol '{}' is already registered", name
            )));
        }
        
        plugins.insert(name, plugin);
        Ok(())
    }
    
    /// 注销插件
    pub fn unregister(&self, name: &str) -> Result<(), TransportError> {
        let mut plugins = self.plugins.write().unwrap();
        if plugins.remove(name).is_some() {
            Ok(())
        } else {
            Err(TransportError::config_error("plugin", format!(
                "Protocol '{}' is not registered", name
            )))
        }
    }
    
    /// 获取插件
    pub fn get(&self, name: &str) -> Option<Arc<dyn ProtocolPlugin>> {
        let plugins = self.plugins.read().unwrap();
        plugins.get(name).cloned()
    }
    
    /// 列出所有插件
    pub fn list(&self) -> Vec<String> {
        let plugins = self.plugins.read().unwrap();
        plugins.keys().cloned().collect()
    }
    
    /// 获取插件信息
    pub fn get_plugin_info(&self, name: &str) -> Option<PluginInfo> {
        let plugins = self.plugins.read().unwrap();
        plugins.get(name).map(|plugin| PluginInfo {
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            features: plugin.features(),
            default_port: plugin.default_port(),
            config_type: plugin.config_type_name().to_string(),
        })
    }
    
    /// 获取所有插件信息
    pub fn get_all_plugin_info(&self) -> Vec<PluginInfo> {
        let plugins = self.plugins.read().unwrap();
        plugins.values().map(|plugin| PluginInfo {
            name: plugin.name().to_string(),
            description: plugin.description().to_string(),
            features: plugin.features(),
            default_port: plugin.default_port(),
            config_type: plugin.config_type_name().to_string(),
        }).collect()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// 插件信息
#[derive(Debug, Clone)]
pub struct PluginInfo {
    /// 插件名称
    pub name: String,
    /// 插件描述
    pub description: String,
    /// 插件特性
    pub features: Vec<String>,
    /// 默认端口
    pub default_port: Option<u16>,
    /// 配置类型名称
    pub config_type: String,
}

/// 插件管理器
pub struct PluginManager {
    registry: PluginRegistry,
    /// 是否允许覆盖内置协议
    allow_override: bool,
}

impl PluginManager {
    /// 创建新的插件管理器
    pub fn new() -> Self {
        Self {
            registry: PluginRegistry::new(),
            allow_override: false,
        }
    }
    
    /// 创建允许覆盖的插件管理器
    pub fn with_override() -> Self {
        Self {
            registry: PluginRegistry::new(),
            allow_override: true,
        }
    }
    
    /// 注册插件
    pub fn register_plugin(&self, plugin: Arc<dyn ProtocolPlugin>) -> Result<(), TransportError> {
        let name = plugin.name();
        
        // 检查是否为内置协议
        if !self.allow_override && self.is_builtin_protocol(name) {
            return Err(TransportError::config_error("plugin", format!(
                "Cannot override builtin protocol '{}'. Use with_override() to allow this.", name
            )));
        }
        
        self.registry.register(plugin)
    }
    
    /// 注销插件
    pub fn unregister_plugin(&self, name: &str) -> Result<(), TransportError> {
        self.registry.unregister(name)
    }
    
    /// 获取插件
    pub fn get_plugin(&self, name: &str) -> Option<Arc<dyn ProtocolPlugin>> {
        self.registry.get(name)
    }
    
    /// 列出所有协议（包括内置和插件）
    pub fn list_protocols(&self) -> Vec<String> {
        let mut protocols = vec!["tcp".to_string(), "websocket".to_string(), "quic".to_string()];
        protocols.extend(self.registry.list());
        protocols.sort();
        protocols.dedup();
        protocols
    }
    
    /// 获取协议信息
    pub fn get_protocol_info(&self, name: &str) -> Option<PluginInfo> {
        // 先检查插件
        if let Some(info) = self.registry.get_plugin_info(name) {
            return Some(info);
        }
        
        // 再检查内置协议
        match name {
            "tcp" => Some(PluginInfo {
                name: "tcp".to_string(),
                description: "Transmission Control Protocol - reliable, ordered, connection-oriented".to_string(),
                features: vec!["reliable".to_string(), "ordered".to_string(), "connection-oriented".to_string()],
                default_port: Some(8080),
                config_type: "TcpConfig".to_string(),
            }),
            "websocket" => Some(PluginInfo {
                name: "websocket".to_string(),
                description: "WebSocket Protocol - full-duplex communication over HTTP".to_string(),
                features: vec!["full-duplex".to_string(), "http-upgrade".to_string(), "frame-based".to_string()],
                default_port: Some(8080),
                config_type: "WebSocketConfig".to_string(),
            }),
            "quic" => Some(PluginInfo {
                name: "quic".to_string(),
                description: "QUIC Protocol - modern transport with built-in encryption".to_string(),
                features: vec!["encrypted".to_string(), "multiplexed".to_string(), "low-latency".to_string()],
                default_port: Some(4433),
                config_type: "QuicConfig".to_string(),
            }),
            _ => None,
        }
    }
    
    /// 检查是否为内置协议
    fn is_builtin_protocol(&self, name: &str) -> bool {
        matches!(name, "tcp" | "websocket" | "quic")
    }
    
    /// 创建带插件的服务器
    pub async fn create_server_with_plugin(
        &self, 
        protocol: &str, 
        config: Box<dyn std::any::Any>
    ) -> Result<Box<dyn Server<Connection = Box<dyn Connection>>>, TransportError> {
        if let Some(plugin) = self.get_plugin(protocol) {
            plugin.create_server(config).await
        } else {
            Err(TransportError::config_error("plugin", format!(
                "Protocol '{}' not found. Available protocols: {:?}", 
                protocol, 
                self.list_protocols()
            )))
        }
    }
    
    /// 创建带插件的连接工厂
    pub async fn create_connection_factory_with_plugin(
        &self,
        protocol: &str,
        config: Box<dyn std::any::Any>
    ) -> Result<Box<dyn ConnectionFactory<Connection = Box<dyn Connection>>>, TransportError> {
        if let Some(plugin) = self.get_plugin(protocol) {
            plugin.create_connection_factory(config).await
        } else {
            Err(TransportError::config_error("plugin", format!(
                "Protocol '{}' not found. Available protocols: {:?}", 
                protocol, 
                self.list_protocols()
            )))
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// 用于插件的连接包装器
pub struct PluginConnection {
    inner: Box<dyn Connection>,
}

impl PluginConnection {
    pub fn new(inner: Box<dyn Connection>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl Connection for PluginConnection {
    async fn send(&mut self, packet: crate::packet::Packet) -> Result<(), TransportError> {
        self.inner.send(packet).await
    }
    
    async fn receive(&mut self) -> Result<Option<crate::packet::Packet>, TransportError> {
        self.inner.receive().await
    }
    
    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }
    
    fn session_id(&self) -> crate::SessionId {
        self.inner.session_id()
    }
    
    fn info(&self) -> &crate::command::ConnectionInfo {
        self.inner.info()
    }
    
    fn is_active(&self) -> bool {
        self.inner.is_active()
    }
    
    async fn flush(&mut self) -> Result<(), TransportError> {
        self.inner.flush().await
    }
} 