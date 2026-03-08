//! OpenFang Scheduler - 定时任务调度系统
//!
//! 提供 Cron 表达式解析、任务调度、状态管理等功能

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

pub mod cron;
pub mod engine;
pub mod store;

pub use cron::CronParser;
pub use engine::SchedulerEngine;
pub use store::JobStore;

/// 调度器错误类型
#[derive(Error, Debug)]
pub enum SchedulerError {
    #[error("Invalid cron expression: {0}")]
    InvalidCron(String),
    #[error("Job not found: {0}")]
    JobNotFound(String),
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Engine error: {0}")]
    EngineError(String),
}

/// 任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    /// 已调度，等待执行
    Scheduled,
    /// 正在执行
    Running,
    /// 执行成功
    Succeeded,
    /// 执行失败（可重试）
    Failed { retry_count: u32 },
    /// 死信队列（最终失败）
    DeadLetter,
    /// 已暂停
    Paused,
    /// 已取消
    Cancelled,
}

impl Default for JobStatus {
    fn default() -> Self {
        JobStatus::Scheduled
    }
}

/// 触发器类型
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerType {
    /// Cron 表达式触发
    Cron {
        expression: String,
        timezone: String,
    },
    /// 间隔触发（秒）
    Interval {
        seconds: u64,
        jitter_seconds: Option<u64>,
    },
    /// 事件触发
    Event {
        pattern: String,
    },
    /// 条件触发
    Condition {
        metric: String,
        threshold: f64,
        operator: String,
    },
}

/// 重试策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay_secs: u64,
    pub backoff_multiplier: f64,
    pub max_delay_secs: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay_secs: 60,
            backoff_multiplier: 2.0,
            max_delay_secs: 3600,
        }
    }
}

/// 定时任务定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: String,
    pub name: String,
    pub hand_id: String,
    pub trigger: TriggerType,
    pub status: JobStatus,
    pub retry_policy: RetryPolicy,
    pub next_run_at: Option<DateTime<Utc>>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub metadata: HashMap<String, serde_json::Value>,
}

impl ScheduledJob {
    pub fn new(name: impl Into<String>, hand_id: impl Into<String>, trigger: TriggerType) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            hand_id: hand_id.into(),
            trigger,
            status: JobStatus::Scheduled,
            retry_policy: RetryPolicy::default(),
            next_run_at: None,
            last_run_at: None,
            created_at: now,
            updated_at: now,
            metadata: HashMap::new(),
        }
    }

    pub fn with_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.retry_policy = policy;
        self
    }
}

/// 任务执行记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobExecution {
    pub id: String,
    pub job_id: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: JobStatus,
    pub output: Option<String>,
    pub error_message: Option<String>,
    pub fuel_consumed: Option<u64>,
}

impl JobExecution {
    pub fn new(job_id: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            job_id: job_id.into(),
            started_at: Utc::now(),
            completed_at: None,
            status: JobStatus::Running,
            output: None,
            error_message: None,
            fuel_consumed: None,
        }
    }

    pub fn complete(mut self, status: JobStatus, output: Option<String>) -> Self {
        self.completed_at = Some(Utc::now());
        self.status = status;
        self.output = output;
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error_message = Some(error.into());
        self.status = JobStatus::DeadLetter;
        self.completed_at = Some(Utc::now());
        self
    }
}

/// 调度器接口
#[async_trait]
pub trait Scheduler: Send + Sync {
    /// 创建定时任务
    async fn create_job(&self, job: ScheduledJob) -> Result<ScheduledJob, SchedulerError>;

    /// 获取任务
    async fn get_job(&self, id: &str) -> Result<Option<ScheduledJob>, SchedulerError>;

    /// 列出所有任务
    async fn list_jobs(&self) -> Result<Vec<ScheduledJob>, SchedulerError>;

    /// 更新任务
    async fn update_job(&self, job: ScheduledJob) -> Result<ScheduledJob, SchedulerError>;

    /// 删除任务
    async fn delete_job(&self, id: &str) -> Result<(), SchedulerError>;

    /// 控制任务状态 (pause/resume/trigger/cancel)
    async fn control_job(
        &self,
        id: &str,
        action: ControlAction,
    ) -> Result<ScheduledJob, SchedulerError>;

    /// 获取任务执行历史
    async fn get_job_history(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<JobExecution>, SchedulerError>;
}

/// 控制动作
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAction {
    Pause,
    Resume,
    Trigger,
    Cancel,
}

impl std::str::FromStr for ControlAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pause" => Ok(ControlAction::Pause),
            "resume" => Ok(ControlAction::Resume),
            "trigger" => Ok(ControlAction::Trigger),
            "cancel" => Ok(ControlAction::Cancel),
            _ => Err(format!("Unknown control action: {}", s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_job_creation() {
        let trigger = TriggerType::Cron {
            expression: "0 0 * * *".to_string(),
            timezone: "UTC".to_string(),
        };
        let job = ScheduledJob::new("test_job", "hand_123", trigger);

        assert_eq!(job.name, "test_job");
        assert_eq!(job.hand_id, "hand_123");
        assert!(matches!(job.status, JobStatus::Scheduled));
    }

    #[test]
    fn test_control_action_parsing() {
        assert_eq!(
            "pause".parse::<ControlAction>().unwrap(),
            ControlAction::Pause
        );
        assert_eq!(
            "RESUME".parse::<ControlAction>().unwrap(),
            ControlAction::Resume
        );
        assert_eq!(
            "Trigger".parse::<ControlAction>().unwrap(),
            ControlAction::Trigger
        );
        assert!("invalid".parse::<ControlAction>().is_err());
    }
}
