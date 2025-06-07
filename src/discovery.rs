use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, Duration};
use async_trait::async_trait;
use crate::error::TransportError;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 服务实例信息
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// 服务ID
    pub id: String,
    /// 服务名称
    pub name: String,
    /// 服务地址
    pub address: std::net::SocketAddr,
    /// 协议类型
    pub protocol: String,
    /// 健康状态
    pub healthy: bool,
    /// 元数据
    pub metadata: HashMap<String, String>,
    /// 注册时间
    pub registered_at: SystemTime,
    /// 最后心跳时间
    pub last_heartbeat: SystemTime,
    /// 权重（用于负载均衡）
    pub weight: u32,
}

impl ServiceInstance {
    /// 创建新的服务实例
    pub fn new(id: String, name: String, address: std::net::SocketAddr, protocol: String) -> Self {
        let now = SystemTime::now();
        Self {
            id,
            name,
            address,
            protocol,
            healthy: true,
            metadata: HashMap::new(),
            registered_at: now,
            last_heartbeat: now,
            weight: 100, // 默认权重
        }
    }
    
    /// 添加元数据
    pub fn with_metadata(mut self, key: String, value: String) -> Self {
        self.metadata.insert(key, value);
        self
    }
    
    /// 设置权重
    pub fn with_weight(mut self, weight: u32) -> Self {
        self.weight = weight;
        self
    }
    
    /// 更新心跳
    pub fn update_heartbeat(&mut self) {
        self.last_heartbeat = SystemTime::now();
        self.healthy = true;
    }
    
    /// 检查是否过期
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.last_heartbeat.elapsed().unwrap_or(Duration::MAX) > timeout
    }
}

/// 服务发现接口
#[async_trait]
pub trait ServiceDiscovery: Send + Sync {
    /// 注册服务
    async fn register(&self, instance: ServiceInstance) -> Result<(), TransportError>;
    
    /// 注销服务
    async fn deregister(&self, service_id: &str) -> Result<(), TransportError>;
    
    /// 发现服务
    async fn discover(&self, service_name: &str) -> Result<Vec<ServiceInstance>, TransportError>;
    
    /// 获取所有服务
    async fn list_services(&self) -> Result<Vec<String>, TransportError>;
    
    /// 更新服务健康状态
    async fn update_health(&self, service_id: &str, healthy: bool) -> Result<(), TransportError>;
    
    /// 发送心跳
    async fn heartbeat(&self, service_id: &str) -> Result<(), TransportError>;
}

/// 内存服务发现实现
pub struct InMemoryServiceDiscovery {
    services: Arc<RwLock<HashMap<String, ServiceInstance>>>,
    heartbeat_timeout: Duration,
}

impl InMemoryServiceDiscovery {
    /// 创建新的内存服务发现
    pub fn new() -> Self {
        Self::with_timeout(Duration::from_secs(30))
    }
    
    /// 创建带超时的内存服务发现
    pub fn with_timeout(timeout: Duration) -> Self {
        Self {
            services: Arc::new(RwLock::new(HashMap::new())),
            heartbeat_timeout: timeout,
        }
    }
    
    /// 清理过期服务
    pub fn cleanup_expired(&self) {
        let mut services = self.services.write().unwrap();
        services.retain(|_, instance| !instance.is_expired(self.heartbeat_timeout));
    }
    
    /// 按权重选择服务实例
    pub fn select_by_weight<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        
        let total_weight: u32 = instances.iter().map(|i| i.weight).sum();
        if total_weight == 0 {
            return instances.first();
        }
        
        // 简单的权重随机选择 - 使用简单的hash代替随机
        let mut hasher = DefaultHasher::new();
        total_weight.hash(&mut hasher);
        std::ptr::addr_of!(instances).hash(&mut hasher);
        let hash_value = hasher.finish();
        let mut target = (hash_value % total_weight as u64) as u32;
        
        for instance in instances {
            if target < instance.weight {
                return Some(instance);
            }
            target -= instance.weight;
        }
        
        instances.first()
    }
}

impl Default for InMemoryServiceDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceDiscovery for InMemoryServiceDiscovery {
    async fn register(&self, instance: ServiceInstance) -> Result<(), TransportError> {
        let mut services = self.services.write().unwrap();
        services.insert(instance.id.clone(), instance);
        Ok(())
    }
    
    async fn deregister(&self, service_id: &str) -> Result<(), TransportError> {
        let mut services = self.services.write().unwrap();
        services.remove(service_id);
        Ok(())
    }
    
    async fn discover(&self, service_name: &str) -> Result<Vec<ServiceInstance>, TransportError> {
        // 先清理过期服务
        self.cleanup_expired();
        
        let services = self.services.read().unwrap();
        let instances: Vec<ServiceInstance> = services
            .values()
            .filter(|instance| instance.name == service_name && instance.healthy)
            .cloned()
            .collect();
        
        Ok(instances)
    }
    
    async fn list_services(&self) -> Result<Vec<String>, TransportError> {
        let services = self.services.read().unwrap();
        let mut service_names: Vec<String> = services
            .values()
            .map(|instance| instance.name.clone())
            .collect();
        
        service_names.sort();
        service_names.dedup();
        
        Ok(service_names)
    }
    
    async fn update_health(&self, service_id: &str, healthy: bool) -> Result<(), TransportError> {
        let mut services = self.services.write().unwrap();
        if let Some(instance) = services.get_mut(service_id) {
            instance.healthy = healthy;
            if healthy {
                instance.update_heartbeat();
            }
        }
        Ok(())
    }
    
    async fn heartbeat(&self, service_id: &str) -> Result<(), TransportError> {
        let mut services = self.services.write().unwrap();
        if let Some(instance) = services.get_mut(service_id) {
            instance.update_heartbeat();
        }
        Ok(())
    }
}

/// 负载均衡策略
#[derive(Debug, Clone)]
pub enum LoadBalanceStrategy {
    /// 轮询
    RoundRobin,
    /// 随机
    Random,
    /// 权重随机
    WeightedRandom,
    /// 最少连接
    LeastConnections,
}

/// 负载均衡器
pub struct LoadBalancer {
    strategy: LoadBalanceStrategy,
    round_robin_index: Arc<RwLock<usize>>,
}

impl LoadBalancer {
    /// 创建新的负载均衡器
    pub fn new(strategy: LoadBalanceStrategy) -> Self {
        Self {
            strategy,
            round_robin_index: Arc::new(RwLock::new(0)),
        }
    }
    
    /// 选择服务实例
    pub fn select<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance> {
        if instances.is_empty() {
            return None;
        }
        
        // 过滤健康的实例
        let healthy_instances: Vec<&ServiceInstance> = instances
            .iter()
            .filter(|instance| instance.healthy)
            .collect();
        
        if healthy_instances.is_empty() {
            return None;
        }
        
        match self.strategy {
            LoadBalanceStrategy::RoundRobin => {
                let mut index = self.round_robin_index.write().unwrap();
                let selected = healthy_instances[*index % healthy_instances.len()];
                *index = (*index + 1) % healthy_instances.len();
                Some(selected)
            }
            LoadBalanceStrategy::Random => {
                let hash_value = std::ptr::addr_of!(healthy_instances) as usize;
                let index = hash_value % healthy_instances.len();
                Some(healthy_instances[index])
            }
            LoadBalanceStrategy::WeightedRandom => {
                // 简化实现，直接从原始instances中选择第一个健康的高权重实例
                healthy_instances
                    .into_iter()
                    .max_by_key(|instance| instance.weight)
            }
            LoadBalanceStrategy::LeastConnections => {
                // 简化实现：选择权重最高的（假设权重反映连接负载）
                healthy_instances
                    .into_iter()
                    .max_by_key(|instance| instance.weight)
            }
        }
    }
} 