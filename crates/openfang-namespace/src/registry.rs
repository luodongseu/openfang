//! 命名空间注册表
//!
//! 管理多个命名空间的生命周期和隔离

use crate::{Namespace, NamespaceConfig, NamespaceError, NamespaceManager, ResourceQuota};
use async_trait::async_trait;
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

/// 命名空间注册表
pub struct NamespaceRegistry {
    /// 内存中的命名空间缓存
    namespaces: Arc<DashMap<String, Arc<Namespace>>>,
    /// 基础存储目录
    base_dir: PathBuf,
    /// 资源使用统计
    usage_stats: Arc<DashMap<String, DashMap<String, u64>>>,
}

impl NamespaceRegistry {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            namespaces: Arc::new(DashMap::new()),
            base_dir,
            usage_stats: Arc::new(DashMap::new()),
        }
    }

    /// 初始化默认命名空间
    pub async fn init_default(&self) -> Result<(), NamespaceError> {
        if !self.namespaces.contains_key("default") {
            let default_ns = Arc::new(Namespace::new("default", "Default Namespace"));
            self.namespaces.insert("default".to_string(), default_ns);

            // 创建默认命名空间目录
            let ns_path = self.base_dir.join("namespaces").join("default");
            tokio::fs::create_dir_all(&ns_path)
                .await
                .map_err(|e| NamespaceError::StoreError(e.to_string()))?;

            info!("Initialized default namespace");
        }
        Ok(())
    }

    /// 获取命名空间存储路径
    pub fn namespace_path(&self, namespace_id: &str) -> PathBuf {
        self.base_dir.join("namespaces").join(namespace_id)
    }

    /// 确保命名空间目录存在
    async fn ensure_namespace_dir(&self, namespace_id: &str) -> Result<PathBuf, NamespaceError> {
        let path = self.namespace_path(namespace_id);
        tokio::fs::create_dir_all(&path)
            .await
            .map_err(|e| NamespaceError::StoreError(e.to_string()))?;
        Ok(path)
    }

    /// 获取或创建资源使用统计
    fn get_or_create_usage(&self, namespace_id: &str) -> Arc<DashMap<String, u64>> {
        if let Some(usage) = self.usage_stats.get(namespace_id) {
            Arc::new(usage.value().clone())
        } else {
            let new_usage = Arc::new(DashMap::new());
            self.usage_stats.insert(namespace_id.to_string(), (*new_usage).clone());
            new_usage
        }
    }
}

#[async_trait]
impl NamespaceManager for NamespaceRegistry {
    async fn create_namespace(
        &self,
        id: impl Into<String> + Send,
        display_name: impl Into<String> + Send,
    ) -> Result<Namespace, NamespaceError> {
        let id = id.into();
        let display_name = display_name.into();

        if self.namespaces.contains_key(&id) {
            return Err(NamespaceError::AlreadyExists(id));
        }

        // 创建命名空间目录
        self.ensure_namespace_dir(&id).await?;

        let namespace = Namespace::new(&id, &display_name);
        let arc_ns = Arc::new(namespace.clone());

        self.namespaces.insert(id.clone(), arc_ns);

        info!("Created namespace: {} ({})", display_name, id);

        Ok(namespace)
    }

    async fn get_namespace(&self, id: &str) -> Result<Option<Namespace>, NamespaceError> {
        Ok(self.namespaces.get(id).map(|ns| (**ns.value()).clone()))
    }

    async fn list_namespaces(&self) -> Result<Vec<Namespace>, NamespaceError> {
        Ok(self
            .namespaces
            .iter()
            .map(|entry| (**entry.value()).clone())
            .collect())
    }

    async fn update_namespace(&self, namespace: Namespace) -> Result<Namespace, NamespaceError> {
        let id = namespace.id.clone();
        let arc_ns = Arc::new(namespace);

        self.namespaces.insert(id.clone(), arc_ns.clone());

        info!("Updated namespace: {}", id);

        Ok((*arc_ns).clone())
    }

    async fn delete_namespace(&self, id: &str) -> Result<(), NamespaceError> {
        if id == "default" {
            return Err(NamespaceError::PermissionDenied(
                "Cannot delete default namespace".to_string(),
            ));
        }

        // 清理资源使用统计
        self.usage_stats.remove(id);

        // 从内存中移除
        self.namespaces.remove(id);

        // 可选：删除存储目录
        let path = self.namespace_path(id);
        if path.exists() {
            tokio::fs::remove_dir_all(&path)
                .await
                .map_err(|e| NamespaceError::StoreError(e.to_string()))?;
        }

        info!("Deleted namespace: {}", id);

        Ok(())
    }

    async fn check_quota(
        &self,
        namespace_id: &str,
        resource: &str,
        requested: u64,
    ) -> Result<bool, NamespaceError> {
        let namespace = self
            .get_namespace(namespace_id)
            .await?
            .ok_or_else(|| NamespaceError::NotFound(namespace_id.to_string()))?;

        let quota_limit = namespace.quota.get_limit(resource);
        let current_usage = self
            .get_or_create_usage(namespace_id)
            .get(resource)
            .map(|u| *u.value())
            .unwrap_or(0);

        let allowed = current_usage + requested <= quota_limit;

        if !allowed {
            warn!(
                "Quota exceeded for namespace {}: resource={}, requested={}, current={}, limit={}",
                namespace_id, resource, requested, current_usage, quota_limit
            );
        }

        Ok(allowed)
    }

    async fn update_usage(
        &self,
        namespace_id: &str,
        resource: &str,
        delta: i64,
    ) -> Result<(), NamespaceError> {
        let usage = self.get_or_create_usage(namespace_id);

        usage
            .entry(resource.to_string())
            .and_modify(|v| {
                if delta >= 0 {
                    *v += delta as u64;
                } else {
                    *v = v.saturating_sub((-delta) as u64);
                }
            })
            .or_insert_with(|| if delta > 0 { delta as u64 } else { 0 });

        Ok(())
    }

    async fn get_route_bindings(
        &self,
        namespace_id: &str,
    ) -> Result<Vec<crate::RouteBinding>, NamespaceError> {
        let namespace = self
            .get_namespace(namespace_id)
            .await?
            .ok_or_else(|| NamespaceError::NotFound(namespace_id.to_string()))?;

        Ok(namespace.config.route_bindings)
    }

    async fn add_route_binding(
        &self,
        namespace_id: &str,
        binding: crate::RouteBinding,
    ) -> Result<(), NamespaceError> {
        let mut namespace = self
            .get_namespace(namespace_id)
            .await?
            .ok_or_else(|| NamespaceError::NotFound(namespace_id.to_string()))?;

        namespace.config.route_bindings.push(binding);
        namespace.updated_at = chrono::Utc::now();

        self.update_namespace(namespace).await?;

        Ok(())
    }

    async fn remove_route_binding(
        &self,
        namespace_id: &str,
        binding_id: &str,
    ) -> Result<(), NamespaceError> {
        let mut namespace = self
            .get_namespace(namespace_id)
            .await?
            .ok_or_else(|| NamespaceError::NotFound(namespace_id.to_string()))?;

        namespace
            .config
            .route_bindings
            .retain(|b| b.id != binding_id);
        namespace.updated_at = chrono::Utc::now();

        self.update_namespace(namespace).await?;

        Ok(())
    }

    async fn resolve_route(
        &self,
        namespace_id: &str,
        channel: &str,
        account_id: &str,
        peer_id: Option<&str>,
    ) -> Result<Option<crate::RouteTarget>, NamespaceError> {
        let bindings = self.get_route_bindings(namespace_id).await?;

        // 按优先级排序（数字越小优先级越高）
        let mut sorted_bindings: Vec<_> = bindings.into_iter().collect();
        sorted_bindings.sort_by_key(|b| b.priority);

        // 查找第一个匹配的绑定
        for binding in sorted_bindings {
            if binding.matches(channel, account_id, peer_id) {
                return Ok(Some(binding.target));
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_registry_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let registry = NamespaceRegistry::new(temp_dir.path().to_path_buf());

        // 初始化默认命名空间
        registry.init_default().await.unwrap();

        // 创建命名空间
        let ns = registry
            .create_namespace("test", "Test Namespace")
            .await
            .unwrap();
        assert_eq!(ns.id, "test");

        // 获取命名空间
        let retrieved = registry.get_namespace("test").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().display_name, "Test Namespace");

        // 列出命名空间
        let namespaces = registry.list_namespaces().await.unwrap();
        assert_eq!(namespaces.len(), 2); // default + test

        // 删除命名空间
        registry.delete_namespace("test").await.unwrap();
        let retrieved = registry.get_namespace("test").await.unwrap();
        assert!(retrieved.is_none());

        // 不能删除默认命名空间
        assert!(registry.delete_namespace("default").await.is_err());
    }

    #[tokio::test]
    async fn test_quota_management() {
        let temp_dir = TempDir::new().unwrap();
        let registry = NamespaceRegistry::new(temp_dir.path().to_path_buf());

        let quota = ResourceQuota::default()
            .with_limit("agents", 10)
            .with_limit("executions_per_minute", 100);

        let ns = Namespace::new("quota_test", "Quota Test").with_quota(quota);
        registry.update_namespace(ns).await.unwrap();

        // 初始检查应该通过
        assert!(registry.check_quota("quota_test", "agents", 5).await.unwrap());

        // 更新使用量
        registry.update_usage("quota_test", "agents", 7).await.unwrap();

        // 现在超过配额
        assert!(!registry.check_quota("quota_test", "agents", 5).await.unwrap());
    }
}
