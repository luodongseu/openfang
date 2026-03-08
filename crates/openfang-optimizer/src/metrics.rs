//! 性能指标收集
//!
//! 记录和分析系统性能数据

use crate::OptimizerError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 指标类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Latency,        // 延迟（毫秒）
    Cost,           // 成本（USD）
    Quality,        // 质量分数（0-100）
    TokenUsage,     // Token 使用量
    SuccessRate,    // 成功率（0-1）
    ErrorRate,      // 错误率（0-1）
    Throughput,     // 吞吐量（请求/分钟）
    ResourceUsage,  // 资源使用率（0-1）
}

impl MetricType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricType::Latency => "latency_ms",
            MetricType::Cost => "cost_usd",
            MetricType::Quality => "quality_score",
            MetricType::TokenUsage => "tokens",
            MetricType::SuccessRate => "success_rate",
            MetricType::ErrorRate => "error_rate",
            MetricType::Throughput => "throughput",
            MetricType::ResourceUsage => "resource_usage",
        }
    }
}

/// 指标数据点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricPoint {
    pub timestamp: DateTime<Utc>,
    pub metric_type: MetricType,
    pub value: f64,
    pub agent_id: Option<String>,
    pub hand_name: Option<String>,
    pub dimensions: HashMap<String, String>,
}

impl MetricPoint {
    pub fn new(metric_type: MetricType, value: f64) -> Self {
        Self {
            timestamp: Utc::now(),
            metric_type,
            value,
            agent_id: None,
            hand_name: None,
            dimensions: HashMap::new(),
        }
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_hand(mut self, hand_name: impl Into<String>) -> Self {
        self.hand_name = Some(hand_name.into());
        self
    }

    pub fn with_dimension(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.dimensions.insert(key.into(), value.into());
        self
    }
}

/// 性能指标汇总
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub namespace_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub metrics: HashMap<MetricType, MetricSummary>,
    pub agent_metrics: HashMap<String, HashMap<MetricType, MetricSummary>>,
}

impl PerformanceMetrics {
    pub fn new(namespace_id: impl Into<String>) -> Self {
        Self {
            namespace_id: namespace_id.into(),
            start_time: Utc::now(),
            end_time: Utc::now(),
            metrics: HashMap::new(),
            agent_metrics: HashMap::new(),
        }
    }

    pub fn add_metric(&mut self, metric_type: MetricType, summary: MetricSummary) {
        self.metrics.insert(metric_type, summary);
    }

    pub fn get_metric(&self, metric_type: MetricType) -> Option<&MetricSummary> {
        self.metrics.get(&metric_type)
    }
}

/// 指标汇总统计
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricSummary {
    pub count: usize,
    pub sum: f64,
    pub mean: f64,
    pub min: f64,
    pub max: f64,
    pub p50: f64,  // 中位数
    pub p95: f64,  // 95 百分位
    pub p99: f64,  // 99 百分位
}

impl MetricSummary {
    pub fn from_values(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self::default();
        }

        let count = values.len();
        let sum: f64 = values.iter().sum();
        let mean = sum / count as f64;

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        Self {
            count,
            sum,
            mean,
            min: *sorted.first().unwrap(),
            max: *sorted.last().unwrap(),
            p50: percentile(&sorted, 0.5),
            p95: percentile(&sorted, 0.95),
            p99: percentile(&sorted, 0.99),
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * p) as usize;
    sorted[index.min(sorted.len() - 1)]
}

/// 指标收集器
pub struct MetricCollector {
    buffer: Arc<RwLock<Vec<MetricPoint>>>,
    max_buffer_size: usize,
    store: Arc<dyn MetricStore>,
}

impl MetricCollector {
    pub fn new(store: Arc<dyn MetricStore>, max_buffer_size: usize) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(Vec::new())),
            max_buffer_size,
            store,
        }
    }

    pub async fn record(&self, point: MetricPoint) -> Result<(), OptimizerError> {
        let mut buffer = self.buffer.write().await;
        buffer.push(point);

        if buffer.len() >= self.max_buffer_size {
            let points: Vec<_> = buffer.drain(..).collect();
            drop(buffer);
            self.store.store_batch(&points).await?;
        }

        Ok(())
    }

    pub async fn flush(&self) -> Result<(), OptimizerError> {
        let mut buffer = self.buffer.write().await;
        if !buffer.is_empty() {
            let points: Vec<_> = buffer.drain(..).collect();
            drop(buffer);
            self.store.store_batch(&points).await?;
        }
        Ok(())
    }

    pub async fn query(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        metric_type: MetricType,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MetricPoint>, OptimizerError> {
        self.store
            .query(namespace_id, agent_id, metric_type, start, end)
            .await
    }

    pub async fn summarize(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        metric_type: MetricType,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<MetricSummary, OptimizerError> {
        let points = self
            .query(namespace_id, agent_id, metric_type, start, end)
            .await?;

        let values: Vec<f64> = points.iter().map(|p| p.value).collect();
        Ok(MetricSummary::from_values(&values))
    }

    pub async fn summarize_performance(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<PerformanceMetrics, OptimizerError> {
        let mut result = PerformanceMetrics::new(namespace_id);
        result.start_time = start;
        result.end_time = end;

        for metric_type in [
            MetricType::Latency,
            MetricType::Cost,
            MetricType::Quality,
            MetricType::SuccessRate,
        ] {
            let summary = self
                .summarize(namespace_id, agent_id, metric_type, start, end)
                .await?;

            if summary.count > 0 {
                result.add_metric(metric_type, summary);
            }
        }

        Ok(result)
    }
}

#[async_trait::async_trait]
pub trait MetricStore: Send + Sync {
    async fn store_batch(&self, points: &[MetricPoint]) -> Result<(), OptimizerError>;

    async fn query(
        &self,
        namespace_id: &str,
        agent_id: Option<&str>,
        metric_type: MetricType,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MetricPoint>, OptimizerError>;

    async fn purge(&self, before: DateTime<Utc>) -> Result<u64, OptimizerError>;
}

pub struct MemoryMetricStore {
    data: Arc<RwLock<Vec<MetricPoint>>>,
}

impl MemoryMetricStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(Vec::new())),
        }
    }
}

#[async_trait::async_trait]
impl MetricStore for MemoryMetricStore {
    async fn store_batch(&self, points: &[MetricPoint]) -> Result<(), OptimizerError> {
        let mut data = self.data.write().await;
        data.extend_from_slice(points);
        Ok(())
    }

    async fn query(
        &self,
        _namespace_id: &str,
        agent_id: Option<&str>,
        metric_type: MetricType,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MetricPoint>, OptimizerError> {
        let data = self.data.read().await;
        let results: Vec<_> = data
            .iter()
            .filter(|p| {
                p.timestamp >= start
                    && p.timestamp <= end
                    && p.metric_type == metric_type
                    && agent_id.map_or(true, |aid| p.agent_id.as_ref() == Some(&aid.to_string()))
            })
            .cloned()
            .collect();
        Ok(results)
    }

    async fn purge(&self, before: DateTime<Utc>) -> Result<u64, OptimizerError> {
        let mut data = self.data.write().await;
        let before_count = data.len();
        data.retain(|p| p.timestamp >= before);
        Ok((before_count - data.len()) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_point_creation() {
        let point = MetricPoint::new(MetricType::Latency, 150.0)
            .with_agent("agent_001")
            .with_dimension("model", "gpt-4o");

        assert_eq!(point.metric_type, MetricType::Latency);
        assert_eq!(point.value, 150.0);
        assert_eq!(point.agent_id, Some("agent_001".to_string()));
    }

    #[test]
    fn test_metric_summary() {
        let values = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let summary = MetricSummary::from_values(&values);

        assert_eq!(summary.count, 5);
        assert_eq!(summary.mean, 3.0);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 5.0);
        assert_eq!(summary.p50, 3.0);
    }

    #[tokio::test]
    async fn test_collector() {
        let store = Arc::new(MemoryMetricStore::new());
        let collector = MetricCollector::new(store, 10);

        for i in 0..5 {
            let point = MetricPoint::new(MetricType::Latency, 100.0 + i as f64);
            collector.record(point).await.unwrap();
        }

        collector.flush().await.unwrap();

        let end = Utc::now();
        let start = end - chrono::Duration::hours(1);
        let points = collector
            .query("", None, MetricType::Latency, start, end)
            .await
            .unwrap();

        assert_eq!(points.len(), 5);
    }
}
