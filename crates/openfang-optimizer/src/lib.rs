//! OpenFang Optimizer - 自优化引擎
//!
//! 实现自动 A/B 测试、性能监控和优化决策

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

pub mod ab_test;
pub mod metrics;
pub mod optimizer;

pub use ab_test::{AbTest, AbTestConfig, ExperimentStatus, Variant};
pub use metrics::{MetricCollector, MetricType, PerformanceMetrics};
pub use optimizer::{OptimizerConfig, OptimizerEngine, OptimizationTarget};

/// 优化器错误类型
#[derive(Error, Debug)]
pub enum OptimizerError {
    #[error("Experiment not found: {0}")]
    ExperimentNotFound(String),
    #[error("Invalid experiment configuration: {0}")]
    InvalidConfig(String),
    #[error("Metrics error: {0}")]
    MetricsError(String),
    #[error("Statistical error: {0}")]
    StatsError(String),
    #[error("Store error: {0}")]
    StoreError(String),
}

/// 优化建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub category: OptimizationCategory,
    pub severity: Severity,
    pub title: String,
    pub description: String,
    pub current_value: String,
    pub suggested_value: String,
    pub expected_improvement: ImprovementEstimate,
    pub confidence: f64, // 0.0 - 1.0
    pub auto_applicable: bool,
    pub applied: bool,
    pub applied_at: Option<DateTime<Utc>>,
}

impl OptimizationSuggestion {
    pub fn new(
        category: OptimizationCategory,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            category,
            severity: Severity::Medium,
            title: title.into(),
            description: description.into(),
            current_value: String::new(),
            suggested_value: String::new(),
            expected_improvement: ImprovementEstimate::default(),
            confidence: 0.0,
            auto_applicable: false,
            applied: false,
            applied_at: None,
        }
    }

    pub fn with_current_value(mut self, value: impl Into<String>) -> Self {
        self.current_value = value.into();
        self
    }

    pub fn with_suggestion(mut self, value: impl Into<String>) -> Self {
        self.suggested_value = value.into();
        self
    }

    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = confidence.clamp(0.0, 1.0);
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn auto_applicable(mut self) -> Self {
        self.auto_applicable = true;
        self
    }
}

/// 优化类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationCategory {
    ModelSelection,      // LLM 模型选择优化
    ScheduleFrequency,   // 调度频率优化
    ContextCompression,  // 上下文压缩优化
    HandSelection,       // Hand 选择优化
    TokenUsage,          // Token 使用优化
    CostEfficiency,      // 成本效率优化
    ResponseQuality,     // 响应质量优化
    Latency,             // 延迟优化
    ResourceUsage,       // 资源使用优化
}

/// 严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// 改进估计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImprovementEstimate {
    pub metric: String,
    pub current: f64,
    pub estimated: f64,
    pub unit: String,
}

impl Default for ImprovementEstimate {
    fn default() -> Self {
        Self {
            metric: String::new(),
            current: 0.0,
            estimated: 0.0,
            unit: String::new(),
        }
    }
}

impl ImprovementEstimate {
    pub fn improvement_percent(&self) -> f64 {
        if self.current == 0.0 {
            return 0.0;
        }
        ((self.estimated - self.current) / self.current) * 100.0
    }
}

/// 优化触发条件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerCondition {
    /// 定时触发（每 N 小时）
    Scheduled { interval_hours: u32 },
    /// 指标阈值触发
    MetricThreshold {
        metric: String,
        threshold: f64,
        operator: ThresholdOperator,
    },
    /// 成本预警触发
    CostAlert { budget_percent: f64 },
    /// 用户反馈触发
    UserFeedback { min_rating: f32 },
}

/// 阈值操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThresholdOperator {
    GreaterThan,
    LessThan,
    EqualTo,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

/// 优化器接口
#[async_trait]
pub trait Optimizer: Send + Sync {
    /// 分析当前性能并生成优化建议
    async fn analyze(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError>;

    /// 获取待应用的优化建议
    async fn pending_suggestions(
        &self,
        namespace_id: &str,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError>;

    /// 应用优化建议
    async fn apply_suggestion(
        &self,
        suggestion_id: &str,
    ) -> Result<OptimizationSuggestion, OptimizerError>;

    /// 拒绝优化建议
    async fn reject_suggestion(
        &self,
        suggestion_id: &str,
        reason: &str,
    ) -> Result<(), OptimizerError>;

    /// 创建 A/B 测试
    async fn create_experiment(
        &self,
        config: AbTestConfig,
    ) -> Result<AbTest, OptimizerError>;

    /// 获取实验状态
    async fn get_experiment(
        &self,
        experiment_id: &str,
    ) -> Result<Option<AbTest>, OptimizerError>;

    /// 列出实验
    async fn list_experiments(
        &self,
        namespace_id: &str,
        active_only: bool,
    ) -> Result<Vec<AbTest>, OptimizerError>;

    /// 停止实验
    async fn stop_experiment(
        &self,
        experiment_id: &str,
        winning_variant: Option<String>,
    ) -> Result<AbTest, OptimizerError>;

    /// 获取性能指标
    async fn get_metrics(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<PerformanceMetrics, OptimizerError>;
}

/// 统计结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalResult {
    pub mean_a: f64,
    pub mean_b: f64,
    pub diff: f64,
    pub relative_diff: f64,
    pub p_value: f64,
    pub confidence_interval: (f64, f64),
    pub sample_size_a: usize,
    pub sample_size_b: usize,
    pub significant: bool,
    pub winner: Option<String>,
}

impl StatisticalResult {
    /// 判断统计显著性（p < 0.05 且效应 > 0）
    pub fn is_significant(&self, alpha: f64) -> bool {
        self.p_value < alpha && self.relative_diff > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suggestion_creation() {
        let suggestion = OptimizationSuggestion::new(
            OptimizationCategory::ModelSelection,
            "Switch to cheaper model",
            "Use GPT-4o-mini for simple tasks",
        )
        .with_current_value("GPT-4o")
        .with_suggestion("GPT-4o-mini")
        .with_confidence(0.85)
        .auto_applicable();

        assert_eq!(suggestion.category, OptimizationCategory::ModelSelection);
        assert_eq!(suggestion.confidence, 0.85);
        assert!(suggestion.auto_applicable);
    }

    #[test]
    fn test_improvement_estimate() {
        let estimate = ImprovementEstimate {
            metric: "cost_per_request".to_string(),
            current: 0.10,
            estimated: 0.05,
            unit: "USD".to_string(),
        };

        assert_eq!(estimate.improvement_percent(), -50.0); // 50% reduction
    }

    #[test]
    fn test_severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn test_statistical_result() {
        let result = StatisticalResult {
            mean_a: 100.0,
            mean_b: 110.0,
            diff: 10.0,
            relative_diff: 0.10,
            p_value: 0.03,
            confidence_interval: (0.05, 0.15),
            sample_size_a: 1000,
            sample_size_b: 1000,
            significant: true,
            winner: Some("B".to_string()),
        };

        assert!(result.is_significant(0.05));
    }
}
