//! 资源配额管理
//!
//! 控制命名空间级别的资源限制

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 资源配额定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceQuota {
    /// 最大 Agent 数量
    pub max_agents: u32,
    /// 最大并行执行数
    pub max_concurrent_executions: u32,
    /// 每分钟最大执行次数
    pub max_executions_per_minute: u32,
    /// 存储限制 (MB)
    pub storage_limit_mb: u64,
    /// 每日 LLM Token 限制
    pub daily_token_limit: u64,
    /// 每月预算限制 (USD cents)
    pub monthly_budget_cents: u64,
    /// 自定义限制
    #[serde(flatten)]
    pub custom_limits: HashMap<String, u64>,
}

impl Default for ResourceQuota {
    fn default() -> Self {
        Self {
            max_agents: 10,
            max_concurrent_executions: 5,
            max_executions_per_minute: 60,
            storage_limit_mb: 100,
            daily_token_limit: 1_000_000,
            monthly_budget_cents: 10_000, // $100
            custom_limits: HashMap::new(),
        }
    }
}

impl ResourceQuota {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_limit(mut self, name: impl Into<String>, value: u64) -> Self {
        let name = name.into();
        match name.as_str() {
            "agents" => self.max_agents = value as u32,
            "concurrent_executions" => self.max_concurrent_executions = value as u32,
            "executions_per_minute" => self.max_executions_per_minute = value as u32,
            "storage_mb" => self.storage_limit_mb = value,
            "daily_tokens" => self.daily_token_limit = value,
            "monthly_budget_cents" => self.monthly_budget_cents = value,
            _ => {
                self.custom_limits.insert(name, value);
            }
        }
        self
    }

    /// 获取指定资源的限制值
    pub fn get_limit(&self, resource: &str) -> u64 {
        match resource {
            "agents" => self.max_agents as u64,
            "concurrent_executions" => self.max_concurrent_executions as u64,
            "executions_per_minute" => self.max_executions_per_minute as u64,
            "storage_mb" => self.storage_limit_mb,
            "daily_tokens" => self.daily_token_limit,
            "monthly_budget_cents" => self.monthly_budget_cents,
            _ => self.custom_limits.get(resource).copied().unwrap_or(u64::MAX),
        }
    }

    /// 检查是否在某个资源限制内
    pub fn check(&self, resource: &str, current: u64, delta: u64) -> bool {
        let limit = self.get_limit(resource);
        current + delta <= limit
    }
}

/// 配额限制项
#[derive(Debug, Clone)]
pub struct QuotaLimit {
    pub resource: String,
    pub limit: u64,
    pub current: u64,
    pub unit: String,
}

impl QuotaLimit {
    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.current)
    }

    pub fn utilization_percent(&self) -> f64 {
        if self.limit == 0 {
            100.0
        } else {
            (self.current as f64 / self.limit as f64) * 100.0
        }
    }
}

/// 配额使用报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaReport {
    pub namespace_id: String,
    pub limits: Vec<QuotaLimitItem>,
    pub report_time: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaLimitItem {
    pub resource: String,
    pub limit: u64,
    pub current: u64,
    pub unit: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_quota() {
        let quota = ResourceQuota::default();
        assert_eq!(quota.max_agents, 10);
        assert_eq!(quota.daily_token_limit, 1_000_000);
    }

    #[test]
    fn test_custom_quota() {
        let quota = ResourceQuota::new()
            .with_limit("agents", 20)
            .with_limit("storage_mb", 500)
            .with_limit("custom_metric", 100);

        assert_eq!(quota.max_agents, 20);
        assert_eq!(quota.storage_limit_mb, 500);
        assert_eq!(quota.get_limit("custom_metric"), 100);
    }

    #[test]
    fn test_quota_check() {
        let quota = ResourceQuota::new().with_limit("agents", 10);

        assert!(quota.check("agents", 5, 3)); // 5 + 3 = 8 <= 10
        assert!(!quota.check("agents", 8, 5)); // 8 + 5 = 13 > 10
    }

    #[test]
    fn test_quota_limit_utilization() {
        let limit = QuotaLimit {
            resource: "agents".to_string(),
            limit: 100,
            current: 75,
            unit: "count".to_string(),
        };

        assert_eq!(limit.remaining(), 25);
        assert_eq!(limit.utilization_percent(), 75.0);
    }
}
