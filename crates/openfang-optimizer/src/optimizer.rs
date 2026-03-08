//! 优化引擎核心
//!
//! 实现自动优化分析和决策

use crate::{
    AbTest, AbTestConfig, ExperimentStatus, MetricCollector, MetricType, OptimizationCategory,
    OptimizationSuggestion, Optimizer, OptimizerError, Severity, TriggerCondition,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};

/// 优化器配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerConfig {
    /// 优化周期（小时）
    pub optimization_interval_hours: u32,
    /// 触发条件
    pub trigger_conditions: Vec<TriggerCondition>,
    /// 优化目标
    pub targets: Vec<OptimizationTarget>,
    /// 自动应用低风险优化
    pub auto_apply_low_risk: bool,
    /// 置信度阈值
    pub confidence_threshold: f64,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            optimization_interval_hours: 6,
            trigger_conditions: vec![
                TriggerCondition::Scheduled {
                    interval_hours: 6,
                },
                TriggerCondition::CostAlert {
                    budget_percent: 80.0,
                },
            ],
            targets: vec![
                OptimizationTarget::ModelSelection,
                OptimizationTarget::CostEfficiency,
                OptimizationTarget::Latency,
            ],
            auto_apply_low_risk: false,
            confidence_threshold: 0.8,
        }
    }
}

/// 优化目标
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationTarget {
    ModelSelection,
    ScheduleFrequency,
    ContextCompression,
    HandSelection,
    TokenUsage,
    CostEfficiency,
    ResponseQuality,
    Latency,
    ResourceUsage,
}

impl OptimizationTarget {
    pub fn category(&self) -> OptimizationCategory {
        match self {
            OptimizationTarget::ModelSelection => OptimizationCategory::ModelSelection,
            OptimizationTarget::ScheduleFrequency => OptimizationCategory::ScheduleFrequency,
            OptimizationTarget::ContextCompression => OptimizationCategory::ContextCompression,
            OptimizationTarget::HandSelection => OptimizationCategory::HandSelection,
            OptimizationTarget::TokenUsage => OptimizationCategory::TokenUsage,
            OptimizationTarget::CostEfficiency => OptimizationCategory::CostEfficiency,
            OptimizationTarget::ResponseQuality => OptimizationCategory::ResponseQuality,
            OptimizationTarget::Latency => OptimizationCategory::Latency,
            OptimizationTarget::ResourceUsage => OptimizationCategory::ResourceUsage,
        }
    }
}

/// 优化引擎
pub struct OptimizerEngine {
    config: OptimizerConfig,
    suggestions: Arc<DashMap<String, OptimizationSuggestion>>,
    experiments: Arc<DashMap<String, AbTest>>,
    metrics: Arc<MetricCollector>,
    running: Arc<std::sync::atomic::AtomicBool>,
}

impl OptimizerEngine {
    pub fn new(config: OptimizerConfig, metrics: Arc<MetricCollector>) -> Self {
        Self {
            config,
            suggestions: Arc::new(DashMap::new()),
            experiments: Arc::new(DashMap::new()),
            metrics,
            running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// 启动优化引擎
    pub async fn start(&self) {
        if self
            .running
            .swap(true, std::sync::atomic::Ordering::SeqCst)
        {
            return;
        }

        info!("Optimizer engine started");

        let interval = Duration::hours(self.config.optimization_interval_hours as i64);
        let running = self.running.clone();

        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(tokio::time::Duration::from_secs(
                interval.num_seconds() as u64,
            ));

            while running.load(std::sync::atomic::Ordering::SeqCst) {
                ticker.tick().await;
                // 触发优化分析
            }
        });
    }

    /// 停止优化引擎
    pub fn stop(&self) {
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        info!("Optimizer engine stopped");
    }

    /// 分析模型选择优化
    async fn analyze_model_selection(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError> {
        let mut suggestions = Vec::new();
        let end = Utc::now();
        let start = end - Duration::hours(24);

        // 获取成本指标
        let cost_summary = self
            .metrics
            .summarize(namespace_id, agent_id, MetricType::Cost, start, end)
            .await?;

        // 获取质量指标
        let quality_summary = self
            .metrics
            .summarize(namespace_id, agent_id, MetricType::Quality, start, end)
            .await?;

        // 如果成本高但质量一般，建议切换模型
        if cost_summary.mean > 0.05 && quality_summary.mean > 85.0 {
            let suggestion = OptimizationSuggestion::new(
                OptimizationCategory::ModelSelection,
                "Switch to GPT-4o-mini for cost savings",
                "Current quality is high enough to use a cheaper model",
            )
            .with_current_value(&format!("${:.4}/req", cost_summary.mean))
            .with_suggestion("GPT-4o-mini (~50% cost reduction)")
            .with_confidence(0.85)
            .auto_applicable();

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// 分析延迟优化
    async fn analyze_latency(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError> {
        let mut suggestions = Vec::new();
        let end = Utc::now();
        let start = end - Duration::hours(24);

        let latency_summary = self
            .metrics
            .summarize(namespace_id, agent_id, MetricType::Latency, start, end)
            .await?;

        // 如果 P95 延迟过高
        if latency_summary.p95 > 5000.0 {
            let suggestion = OptimizationSuggestion::new(
                OptimizationCategory::Latency,
                "High P95 latency detected",
                "Consider context compression or model downgrade for faster responses",
            )
            .with_current_value(&format!("{:.0}ms P95", latency_summary.p95))
            .with_suggestion("Target <2000ms P95")
            .with_confidence(0.75)
            .with_severity(Severity::High);

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }

    /// 分析成本效率
    async fn analyze_cost_efficiency(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError> {
        let mut suggestions = Vec::new();
        let end = Utc::now();
        let start = end - Duration::hours(24);

        let token_summary = self
            .metrics
            .summarize(namespace_id, agent_id, MetricType::TokenUsage, start, end)
            .await?;

        // 如果 Token 使用量大，建议上下文压缩
        if token_summary.mean > 4000.0 {
            let suggestion = OptimizationSuggestion::new(
                OptimizationCategory::ContextCompression,
                "High token usage detected",
                "Enable context compression to reduce costs",
            )
            .with_current_value(&format!("{:.0} tokens/req", token_summary.mean))
            .with_suggestion("Enable compression (expect 30% reduction)")
            .with_confidence(0.80)
            .auto_applicable();

            suggestions.push(suggestion);
        }

        Ok(suggestions)
    }
}

#[async_trait]
impl Optimizer for OptimizerEngine {
    async fn analyze(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError> {
        let mut all_suggestions = Vec::new();

        // 根据配置的目标进行分析
        for target in &self.config.targets {
            let suggestions = match target {
                OptimizationTarget::ModelSelection => {
                    self.analyze_model_selection(namespace_id, agent_id).await?
                }
                OptimizationTarget::Latency => {
                    self.analyze_latency(namespace_id, agent_id).await?
                }
                OptimizationTarget::CostEfficiency | OptimizationTarget::TokenUsage => {
                    self.analyze_cost_efficiency(namespace_id, agent_id).await?
                }
                _ => Vec::new(), // TODO: 实现其他目标
            };

            all_suggestions.extend(suggestions);
        }

        // 存储建议
        for suggestion in &all_suggestions {
            self.suggestions
                .insert(suggestion.id.clone(), suggestion.clone());
        }

        info!(
            "Generated {} optimization suggestions for namespace {}",
            all_suggestions.len(),
            namespace_id
        );

        Ok(all_suggestions)
    }

    async fn pending_suggestions(
        &self,
        namespace_id: &str,
    ) -> Result<Vec<OptimizationSuggestion>, OptimizerError> {
        // TODO: 添加 namespace 过滤
        let pending: Vec<_> = self
            .suggestions
            .iter()
            .filter(|s| !s.applied)
            .map(|s| s.clone())
            .collect();
        Ok(pending)
    }

    async fn apply_suggestion(
        &self,
        suggestion_id: &str,
    ) -> Result<OptimizationSuggestion, OptimizerError> {
        let mut suggestion = self
            .suggestions
            .get_mut(suggestion_id)
            .ok_or_else(|| OptimizerError::InvalidConfig("Suggestion not found".to_string()))?;

        if suggestion.applied {
            return Err(OptimizerError::InvalidConfig(
                "Suggestion already applied".to_string(),
            ));
        }

        // TODO: 实际应用优化
        suggestion.applied = true;
        suggestion.applied_at = Some(Utc::now());

        info!("Applied optimization suggestion: {}", suggestion_id);

        Ok(suggestion.clone())
    }

    async fn reject_suggestion(
        &self,
        suggestion_id: &str,
        reason: &str,
    ) -> Result<(), OptimizerError> {
        self.suggestions.remove(suggestion_id);
        info!("Rejected suggestion {}: {}", suggestion_id, reason);
        Ok(())
    }

    async fn create_experiment(
        &self,
        config: AbTestConfig,
    ) -> Result<AbTest, OptimizerError> {
        let test = AbTest::new("default", "Auto-experiment", config);
        self.experiments.insert(test.id.clone(), test.clone());
        info!("Created experiment: {}", test.id);
        Ok(test)
    }

    async fn get_experiment(
        &self,
        experiment_id: &str,
    ) -> Result<Option<AbTest>, OptimizerError> {
        Ok(self.experiments.get(experiment_id).map(|t| t.clone()))
    }

    async fn list_experiments(
        &self,
        _namespace_id: &str,
        active_only: bool,
    ) -> Result<Vec<AbTest>, OptimizerError> {
        let experiments: Vec<_> = self
            .experiments
            .iter()
            .filter(|e| !active_only || e.status == ExperimentStatus::Running)
            .map(|e| e.clone())
            .collect();
        Ok(experiments)
    }

    async fn stop_experiment(
        &self,
        experiment_id: &str,
        winning_variant: Option<String>,
    ) -> Result<AbTest, OptimizerError> {
        let mut test = self
            .experiments
            .get_mut(experiment_id)
            .ok_or_else(|| OptimizerError::ExperimentNotFound(experiment_id.to_string()))?;

        test.stop(winning_variant);
        info!("Stopped experiment: {}", experiment_id);

        Ok(test.clone())
    }

    async fn get_metrics(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<crate::PerformanceMetrics, OptimizerError> {
        self.metrics
            .summarize_performance(namespace_id, agent_id, start, end)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MemoryMetricStore;

    fn create_test_engine() -> OptimizerEngine {
        let store = Arc::new(MemoryMetricStore::new());
        let metrics = Arc::new(MetricCollector::new(store, 100));
        OptimizerEngine::new(OptimizerConfig::default(), metrics)
    }

    #[tokio::test]
    async fn test_optimizer_lifecycle() {
        let engine = create_test_engine();

        engine.start().await;
        assert!(engine.running.load(std::sync::atomic::Ordering::SeqCst));

        engine.stop();
        assert!(!engine.running.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[test]
    fn test_optimization_target_mapping() {
        assert_eq!(
            OptimizationTarget::ModelSelection.category(),
            OptimizationCategory::ModelSelection
        );
        assert_eq!(
            OptimizationTarget::Latency.category(),
            OptimizationCategory::Latency
        );
    }
}
