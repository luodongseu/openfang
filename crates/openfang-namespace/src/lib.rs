//! OpenFang Namespace - 多账户/命名空间管理系统
//!
//! 提供隔离的 Agent 注册表、资源配额管理、审计日志等功能

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use uuid::Uuid;

pub mod registry;
pub mod quota;
pub mod audit;
pub mod routing;

pub use registry::NamespaceRegistry;
pub use quota::{QuotaLimit, ResourceQuota};
pub use routing::{RouteBinding, RouteMatcher};

/// 命名空间错误类型
#[derive(Error, Debug)]
pub enum NamespaceError {
    #[error("Namespace not found: {0}")]
    NotFound(String),
    #[error("Namespace already exists: {0}")]
    AlreadyExists(String),
    #[error("Quota exceeded: {resource}")]
    QuotaExceeded { resource: String, limit: u64, current: u64 },
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// 命名空间定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Namespace {
    pub id: String,
    pub display_name: String,
    pub config: NamespaceConfig,
    pub quota: ResourceQuota,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Namespace {
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            display_name: display_name.into(),
            config: NamespaceConfig::default(),
            quota: ResourceQuota::default(),
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn with_config(mut self, config: NamespaceConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_quota(mut self, quota: ResourceQuota) -> Self {
        self.quota = quota;
        self
    }

    /// 获取存储路径
    pub fn storage_path(&self, base_dir: &std::path::Path) -> std::path::PathBuf {
        base_dir.join("namespaces").join(&self.id)
    }

    /// 获取数据库路径
    pub fn database_path(&self, base_dir: &std::path::Path) -> std::path::PathBuf {
        self.storage_path(base_dir).join("memory.db")
    }
}

/// 命名空间配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceConfig {
    /// 是否启用审计日志
    pub audit_enabled: bool,
    /// 审计日志保留天数
    pub audit_retention_days: u32,
    /// 默认 Agent 配置
    pub default_agent_config: HashMap<String, serde_json::Value>,
    /// 允许使用的 Hands
    pub allowed_hands: Option<Vec<String>>,
    /// 禁止使用的 Hands
    pub denied_hands: Vec<String>,
    /// 路由规则
    pub route_bindings: Vec<RouteBinding>,
    /// 渠道配置
    pub channels: HashMap<String, Vec<ChannelAccount>>,
}

impl Default for NamespaceConfig {
    fn default() -> Self {
        Self {
            audit_enabled: true,
            audit_retention_days: 90,
            default_agent_config: HashMap::new(),
            allowed_hands: None,
            denied_hands: vec![],
            route_bindings: vec![],
            channels: HashMap::new(),
        }
    }
}

/// 渠道账户配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelAccount {
    pub account_id: String,
    pub account_type: String,
    #[serde(flatten)]
    pub config: HashMap<String, serde_json::Value>,
    #[serde(skip)]
    pub credential: Option<String>, // 运行时从环境变量或密钥管理获取
}

impl ChannelAccount {
    pub fn new(account_id: impl Into<String>, account_type: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            account_type: account_type.into(),
            config: HashMap::new(),
            credential: None,
        }
    }

    /// 从环境变量获取凭证
    pub fn with_env_credential(mut self, env_var: &str) -> Self {
        self.credential = std::env::var(env_var).ok();
        self
    }
}

/// 命名空间管理器
#[async_trait]
pub trait NamespaceManager: Send + Sync {
    /// 创建命名空间
    async fn create_namespace(
        &self,
        id: impl Into<String> + Send,
        display_name: impl Into<String> + Send,
    ) -> Result<Namespace, NamespaceError>;

    /// 获取命名空间
    async fn get_namespace(&self, id: &str) -> Result<Option<Namespace>, NamespaceError>;

    /// 列出所有命名空间
    async fn list_namespaces(&self) -> Result<Vec<Namespace>, NamespaceError>;

    /// 更新命名空间
    async fn update_namespace(&self, namespace: Namespace) -> Result<Namespace, NamespaceError>;

    /// 删除命名空间
    async fn delete_namespace(&self, id: &str) -> Result<(), NamespaceError>;

    /// 检查资源配额
    async fn check_quota(
        &self,
        namespace_id: &str,
        resource: &str,
        requested: u64,
    ) -> Result<bool, NamespaceError>;

    /// 更新资源使用
    async fn update_usage(
        &self,
        namespace_id: &str,
        resource: &str,
        delta: i64,
    ) -> Result<(), NamespaceError>;

    /// 获取命名空间的路由绑定
    async fn get_route_bindings(
        &self,
        namespace_id: &str,
    ) -> Result<Vec<RouteBinding>, NamespaceError>;

    /// 添加路由绑定
    async fn add_route_binding(
        &self,
        namespace_id: &str,
        binding: RouteBinding,
    ) -> Result<(), NamespaceError>;

    /// 移除路由绑定
    async fn remove_route_binding(
        &self,
        namespace_id: &str,
        binding_id: &str,
    ) -> Result<(), NamespaceError>;

    /// 根据消息路由到目标
    async fn resolve_route(
        &self,
        namespace_id: &str,
        channel: &str,
        account_id: &str,
        peer_id: Option<&str>,
    ) -> Result<Option<RouteTarget>, NamespaceError>;
}

/// 路由目标
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RouteTarget {
    Agent { agent_id: String },
    Hand { hand_id: String },
    Workflow { workflow_id: String },
    Channel { channel: String, account_id: String },
}

impl RouteTarget {
    pub fn agent(agent_id: impl Into<String>) -> Self {
        Self::Agent {
            agent_id: agent_id.into(),
        }
    }

    pub fn hand(hand_id: impl Into<String>) -> Self {
        Self::Hand {
            hand_id: hand_id.into(),
        }
    }
}

/// 全局命名空间上下文（用于运行时）
pub struct NamespaceContext {
    pub namespace: Arc<Namespace>,
    pub agent_registry: Arc<dyn std::any::Any + Send + Sync>,
    pub memory_pool: Arc<dyn std::any::Any + Send + Sync>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_creation() {
        let ns = Namespace::new("personal", "Personal Space")
            .with_config(NamespaceConfig {
                audit_enabled: true,
                audit_retention_days: 30,
                ..Default::default()
            });

        assert_eq!(ns.id, "personal");
        assert_eq!(ns.display_name, "Personal Space");
        assert!(ns.config.audit_enabled);
    }

    #[test]
    fn test_channel_account() {
        let account = ChannelAccount::new("personal_main", "telegram")
            .with_env_credential("TELEGRAM_TOKEN");

        assert_eq!(account.account_id, "personal_main");
        assert_eq!(account.account_type, "telegram");
    }

    #[test]
    fn test_route_target() {
        let target = RouteTarget::agent("assistant_123");
        match target {
            RouteTarget::Agent { agent_id } => assert_eq!(agent_id, "assistant_123"),
            _ => panic!("Expected Agent target"),
        }
    }
}
