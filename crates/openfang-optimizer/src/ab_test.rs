//! A/B 测试框架
//!
//! 支持实验管理、流量分割和统计分析

use crate::{OptimizerError, StatisticalResult};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A/B 测试实验
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTest {
    pub id: String,
    pub namespace_id: String,
    pub name: String,
    pub description: String,
    pub status: ExperimentStatus,
    pub config: AbTestConfig,
    pub variants: Vec<Variant>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub traffic_allocation: TrafficAllocation,
    pub winning_variant: Option<String>,
    pub results: Option<StatisticalResult>,
}

impl AbTest {
    pub fn new(
        namespace_id: impl Into<String>,
        name: impl Into<String>,
        config: AbTestConfig,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();

        Self {
            id,
            namespace_id: namespace_id.into(),
            name: name.into(),
            description: String::new(),
            status: ExperimentStatus::Draft,
            config,
            variants: vec![],
            start_time: now,
            end_time: None,
            created_at: now,
            traffic_allocation: TrafficAllocation::Equal,
            winning_variant: None,
            results: None,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_variants(mut self, variants: Vec<Variant>) -> Self {
        self.variants = variants;
        self
    }

    pub fn add_variant(mut self, variant: Variant) -> Self {
        self.variants.push(variant);
        self
    }

    /// 分配用户到变体（一致性哈希）
    pub fn assign_variant(&self, user_id: &str) -> Option<&Variant> {
        if self.variants.is_empty() {
            return None;
        }

        // 使用一致性哈希
        let hash = Self::hash_user_id(user_id);
        let index = (hash % self.variants.len() as u64) as usize;
        self.variants.get(index)
    }

    /// 简单的哈希函数（用于流量分割）
    fn hash_user_id(user_id: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        user_id.hash(&mut hasher);
        hasher.finish()
    }

    /// 开始实验
    pub fn start(&mut self) {
        if self.status == ExperimentStatus::Draft {
            self.status = ExperimentStatus::Running;
            self.start_time = Utc::now();
        }
    }

    /// 停止实验
    pub fn stop(&mut self, winning_variant: Option<String>) {
        self.status = ExperimentStatus::Completed;
        self.end_time = Some(Utc::now());
        self.winning_variant = winning_variant;
    }

    /// 暂停实验
    pub fn pause(&mut self) {
        if self.status == ExperimentStatus::Running {
            self.status = ExperimentStatus::Paused;
        }
    }

    /// 恢复实验
    pub fn resume(&mut self) {
        if self.status == ExperimentStatus::Paused {
            self.status = ExperimentStatus::Running;
        }
    }

    /// 是否达到最小样本量
    pub fn has_minimum_sample_size(&self) -> bool {
        self.variants
            .iter()
            .all(|v| v.metrics.sample_size >= self.config.min_sample_size)
    }

    /// 是否达到实验持续时间
    pub fn has_reached_duration(&self) -> bool {
        let duration = Utc::now() - self.start_time;
        duration >= Duration::hours(self.config.duration_hours as i64)
    }
}

/// 实验状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExperimentStatus {
    Draft,      // 草稿，未开始
    Running,    // 运行中
    Paused,     // 已暂停
    Completed,  // 已完成
    Cancelled,  // 已取消
}

/// A/B 测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestConfig {
    /// 最小样本量
    pub min_sample_size: usize,
    /// 实验持续时间（小时）
    pub duration_hours: u32,
    /// 统计显著性阈值（alpha）
    pub significance_level: f64,
    /// 主要指标
    pub primary_metric: String,
    /// 次要指标
    pub secondary_metrics: Vec<String>,
    /// 目标提升百分比
    pub target_improvement: f64,
    /// 自动终止
    pub auto_stop: bool,
}

impl Default for AbTestConfig {
    fn default() -> Self {
        Self {
            min_sample_size: 100,
            duration_hours: 168, // 1 week
            significance_level: 0.05,
            primary_metric: "success_rate".to_string(),
            secondary_metrics: vec![],
            target_improvement: 0.05, // 5%
            auto_stop: false,
        }
    }
}

impl AbTestConfig {
    pub fn new(primary_metric: impl Into<String>) -> Self {
        Self {
            primary_metric: primary_metric.into(),
            ..Default::default()
        }
    }

    pub fn with_sample_size(mut self, size: usize) -> Self {
        self.min_sample_size = size;
        self
    }

    pub fn with_duration(mut self, hours: u32) -> Self {
        self.duration_hours = hours;
        self
    }

    pub fn with_significance(mut self, alpha: f64) -> Self {
        self.significance_level = alpha;
        self
    }
}

/// 实验变体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 变体配置（具体配置值）
    pub config: HashMap<String, serde_json::Value>,
    /// 变体指标
    pub metrics: VariantMetrics,
    /// 是否为对照组
    pub is_control: bool,
    /// 流量占比 (0.0 - 1.0)
    pub traffic_percentage: f64,
}

impl Variant {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            description: description.into(),
            config: HashMap::new(),
            metrics: VariantMetrics::default(),
            is_control: false,
            traffic_percentage: 0.5,
        }
    }

    pub fn control(name: impl Into<String>) -> Self {
        Self::new(name, "Control group").with_control(true)
    }

    pub fn with_control(mut self, is_control: bool) -> Self {
        self.is_control = is_control;
        self
    }

    pub fn with_config(
        mut self,
        key: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        self.config.insert(key.into(), serde_json::to_value(value)?);
        Ok(self)
    }

    pub fn with_traffic(mut self, percentage: f64) -> Self {
        self.traffic_percentage = percentage.clamp(0.0, 1.0);
        self
    }

    /// 记录一次成功
    pub fn record_success(&mut self, value: f64) {
        self.metrics.sample_size += 1;
        self.metrics.successes += 1;
        self.metrics.total_value += value;
        self.metrics.update_average(value);
    }

    /// 记录一次失败
    pub fn record_failure(&mut self) {
        self.metrics.sample_size += 1;
        self.metrics.failures += 1;
    }
}

/// 变体指标
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VariantMetrics {
    pub sample_size: usize,
    pub successes: usize,
    pub failures: usize,
    pub total_value: f64,
    pub average_value: f64,
    pub variance: f64,
}

impl VariantMetrics {
    fn update_average(&mut self,
        value: f64,
    ) {
        if self.sample_size == 0 {
            self.average_value = value;
        } else {
            // 增量平均计算
            self.average_value = self.total_value / self.sample_size as f64;
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.sample_size == 0 {
            return 0.0;
        }
        self.successes as f64 / self.sample_size as f64
    }
}

/// 流量分配策略
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficAllocation {
    Equal,         // 均分
    Custom,        // 自定义比例
    Bandit,        // 多臂老虎机（自动优化）
}

/// 实验存储接口
#[async_trait::async_trait]
pub trait ExperimentStore: Send + Sync {
    /// 保存实验
    async fn save_experiment(
        &self,
        experiment: &AbTest,
    ) -> Result<(), OptimizerError>;

    /// 获取实验
    async fn get_experiment(
        &self,
        experiment_id: &str,
    ) -> Result<Option<AbTest>, OptimizerError>;

    /// 列出命名空间的所有实验
    async fn list_experiments(
        &self,
        namespace_id: &str,
        status_filter: Option<ExperimentStatus>,
    ) -> Result<Vec<AbTest>, OptimizerError>;

    /// 记录变体分配
    async fn record_assignment(
        &self,
        experiment_id: &str,
        variant_id: &str,
        user_id: &str,
    ) -> Result<(), OptimizerError>;

    /// 记录变体结果
    async fn record_outcome(
        &self,
        experiment_id: &str,
        variant_id: &str,
        success: bool,
        value: f64,
    ) -> Result<(), OptimizerError>;
}

/// 统计分析工具
pub mod stats {
    use super::*;
    use crate::StatisticalResult;

    /// 计算两个变体的统计显著性（t-test）
    pub fn compare_variants(
        control: &Variant,
        treatment: &Variant,
        alpha: f64,
    ) -> StatisticalResult {
        let n1 = control.metrics.sample_size as f64;
        let n2 = treatment.metrics.sample_size as f64;

        if n1 == 0.0 || n2 == 0.0 {
            return StatisticalResult {
                mean_a: 0.0,
                mean_b: 0.0,
                diff: 0.0,
                relative_diff: 0.0,
                p_value: 1.0,
                confidence_interval: (0.0, 0.0),
                sample_size_a: control.metrics.sample_size,
                sample_size_b: treatment.metrics.sample_size,
                significant: false,
                winner: None,
            };
        }

        let mean1 = control.metrics.average_value;
        let mean2 = treatment.metrics.average_value;

        // 简化计算（实际需要标准差）
        let diff = mean2 - mean1;
        let relative_diff = if mean1 != 0.0 { diff / mean1 } else { 0.0 };

        // 简化的 p-value 计算（实际需要 t-test）
        let p_value = estimate_p_value(control, treatment);

        // 简化的置信区间
        let se = (diff.abs() / 4.0).max(0.001); // 简化估计
        let ci_lower = diff - 1.96 * se;
        let ci_upper = diff + 1.96 * se;

        let significant = p_value < alpha && relative_diff > 0.0;
        let winner = if significant {
            Some(treatment.id.clone())
        } else {
            None
        };

        StatisticalResult {
            mean_a: mean1,
            mean_b: mean2,
            diff,
            relative_diff,
            p_value,
            confidence_interval: (ci_lower, ci_upper),
            sample_size_a: control.metrics.sample_size,
            sample_size_b: treatment.metrics.sample_size,
            significant,
            winner,
        }
    }

    /// 简化的 p-value 估计（实际应该使用适当的统计检验）
    fn estimate_p_value(control: &Variant, treatment: &Variant) -> f64 {
        let diff = (treatment.metrics.average_value - control.metrics.average_value).abs();
        let n = (control.metrics.sample_size + treatment.metrics.sample_size) as f64;

        if n < 10.0 {
            return 1.0; // 样本太小
        }

        // 简化的估计：差异越大，样本越大，p-value 越小
        let effect = diff * (n.sqrt() / 10.0);
        let p = (-effect).exp();
        p.min(0.999).max(0.001)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variant_creation() {
        let control = Variant::control("Control");
        assert!(control.is_control);
        assert_eq!(control.traffic_percentage, 0.5);

        let variant = Variant::new("Treatment", "New feature").with_traffic(0.5);
        assert!(!variant.is_control);
    }

    #[test]
    fn test_ab_test_lifecycle() {
        let config = AbTestConfig::default();
        let mut test = AbTest::new("test_ns", "My Experiment", config)
            .with_description("Testing new model")
            .add_variant(Variant::control("Control"))
            .add_variant(Variant::new("Treatment", "GPT-4o-mini"));

        assert_eq!(test.status, ExperimentStatus::Draft);

        test.start();
        assert_eq!(test.status, ExperimentStatus::Running);

        test.pause();
        assert_eq!(test.status, ExperimentStatus::Paused);

        test.resume();
        assert_eq!(test.status, ExperimentStatus::Running);

        test.stop(Some("treatment_id".to_string()));
        assert_eq!(test.status, ExperimentStatus::Completed);
    }

    #[test]
    fn test_variant_assignment() {
        let config = AbTestConfig::default();
        let test = AbTest::new("test_ns", "Test", config)
            .add_variant(Variant::new("A", "Variant A"))
            .add_variant(Variant::new("B", "Variant B"));

        // 同一用户应该始终分配到同一变体
        let variant1 = test.assign_variant("user_123");
        let variant2 = test.assign_variant("user_123");
        assert_eq!(variant1.unwrap().id, variant2.unwrap().id);

        // 不同用户可能分配到不同变体
        let variant3 = test.assign_variant("user_456");
        assert!(variant3.is_some());
    }

    #[test]
    fn test_variant_metrics() {
        let mut variant = Variant::new("Test", "Test variant");

        variant.record_success(10.0);
        variant.record_success(20.0);
        variant.record_failure();

        assert_eq!(variant.metrics.sample_size, 3);
        assert_eq!(variant.metrics.successes, 2);
        assert_eq!(variant.metrics.failures, 1);
        assert!((variant.metrics.average_value - 15.0).abs() < 0.01);
    }
}
