//! 调度引擎
//!
//! 负责任务的调度、执行和状态管理

use crate::{
    cron::{next_interval_run, CronParser},
    store::JobStore,
    ControlAction, JobExecution, JobStatus, RetryPolicy, ScheduledJob, Scheduler,
    SchedulerError, TriggerType,
};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{debug, error, info, warn};

/// 调度引擎
pub struct SchedulerEngine {
    store: Arc<dyn JobStore>,
    running: Arc<RwLock<bool>>,
    shutdown_tx: broadcast::Sender<()>,
    job_queue: mpsc::Sender<String>, // job_id
}

impl SchedulerEngine {
    pub fn new(store: Arc<dyn JobStore>) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        let (job_queue, _) = mpsc::channel(1000);

        Self {
            store,
            running: Arc::new(RwLock::new(false)),
            shutdown_tx,
            job_queue,
        }
    }

    /// 启动调度引擎
    pub async fn start(&self) -> Result<(), SchedulerError> {
        let mut running = self.running.write().await;
        if *running {
            return Ok(());
        }
        *running = true;
        drop(running);

        info!("Scheduler engine starting...");

        // 启动调度循环
        let store = self.store.clone();
        let running = self.running.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::schedule_pending_jobs(store.as_ref()).await {
                            error!("Failed to schedule pending jobs: {}", e);
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Scheduler loop shutting down");
                        break;
                    }
                }

                if !*running.read().await {
                    break;
                }
            }
        });

        info!("Scheduler engine started");
        Ok(())
    }

    /// 停止调度引擎
    pub async fn stop(&self) {
        let _ = self.shutdown_tx.send(());
        *self.running.write().await = false;
        info!("Scheduler engine stopped");
    }

    /// 调度待执行的任务
    async fn schedule_pending_jobs(store: &dyn JobStore) -> Result<(), SchedulerError> {
        let now = Utc::now();
        let pending_jobs = store.get_pending_jobs(now).await?;

        for job in pending_jobs {
            debug!("Scheduling job: {} ({}) at {:?}", job.name, job.id, now);

            // 更新任务状态为运行中
            let mut job = job;
            job.status = JobStatus::Running;
            job.last_run_at = Some(now);
            store.update_job(&job).await?;

            // 创建执行记录
            let execution = JobExecution::new(&job.id);
            store.save_execution(&execution).await?;

            // 这里会触发实际的 Hand 执行
            // TODO: 调用 Hand 执行器
        }

        Ok(())
    }

    /// 计算下一次执行时间
    pub fn calculate_next_run(job: &ScheduledJob) -> Result<Option<DateTime<Utc>>, SchedulerError> {
        match &job.trigger {
            TriggerType::Cron { expression, timezone } => {
                CronParser::next_run(expression, timezone, Utc::now())
            }
            TriggerType::Interval {
                seconds,
                jitter_seconds,
            } => {
                let next = next_interval_run(*seconds, *jitter_seconds, job.last_run_at);
                Ok(Some(next))
            }
            TriggerType::Event { .. } => {
                // 事件触发器没有固定的下一次执行时间
                Ok(None)
            }
            TriggerType::Condition { .. } => {
                // 条件触发器由监控系统触发
                Ok(None)
            }
        }
    }

    /// 计算下次重试时间
    pub fn calculate_retry_time(policy: &RetryPolicy, retry_count: u32) -> DateTime<Utc> {
        let delay_secs = (policy.initial_delay_secs as f64
            * policy.backoff_multiplier.powi(retry_count as i32))
        .min(policy.max_delay_secs as f64) as i64;

        Utc::now() + Duration::seconds(delay_secs)
    }
}

#[async_trait]
impl Scheduler for SchedulerEngine {
    async fn create_job(
        &self,
        mut job: ScheduledJob,
    ) -> Result<ScheduledJob, SchedulerError> {
        // 计算下一次执行时间
        job.next_run_at = Self::calculate_next_run(&job)?;
        job.updated_at = Utc::now();

        self.store.save_job(&job).await?;
        info!("Created scheduled job: {} ({})", job.name, job.id);

        Ok(job)
    }

    async fn get_job(&self, id: &str) -> Result<Option<ScheduledJob>, SchedulerError> {
        self.store.get_job(id).await
    }

    async fn list_jobs(&self) -> Result<Vec<ScheduledJob>, SchedulerError> {
        self.store.list_jobs().await
    }

    async fn update_job(&self, mut job: ScheduledJob) -> Result<ScheduledJob, SchedulerError> {
        // 重新计算下一次执行时间
        job.next_run_at = Self::calculate_next_run(&job)?;
        job.updated_at = Utc::now();

        self.store.update_job(&job).await?;
        info!("Updated scheduled job: {} ({})", job.name, job.id);

        Ok(job)
    }

    async fn delete_job(&self, id: &str) -> Result<(), SchedulerError> {
        self.store.delete_job(id).await?;
        info!("Deleted scheduled job: {}", id);
        Ok(())
    }

    async fn control_job(
        &self,
        id: &str,
        action: ControlAction,
    ) -> Result<ScheduledJob, SchedulerError> {
        let mut job = self
            .store
            .get_job(id)
            .await?
            .ok_or_else(|| SchedulerError::JobNotFound(id.to_string()))?;

        match action {
            ControlAction::Pause => {
                if matches!(job.status, JobStatus::Running) {
                    return Err(SchedulerError::EngineError(
                        "Cannot pause running job".to_string(),
                    ));
                }
                job.status = JobStatus::Paused;
                info!("Paused job: {}", id);
            }
            ControlAction::Resume => {
                if !matches!(job.status, JobStatus::Paused) {
                    return Err(SchedulerError::EngineError(
                        "Can only resume paused jobs".to_string(),
                    ));
                }
                job.status = JobStatus::Scheduled;
                // 重新计算下一次执行时间
                job.next_run_at = Self::calculate_next_run(&job)?;
                info!("Resumed job: {}", id);
            }
            ControlAction::Trigger => {
                // 立即触发执行
                job.next_run_at = Some(Utc::now());
                job.status = JobStatus::Scheduled;
                info!("Triggered job: {} for immediate execution", id);
            }
            ControlAction::Cancel => {
                job.status = JobStatus::Cancelled;
                job.next_run_at = None;
                info!("Cancelled job: {}", id);
            }
        }

        job.updated_at = Utc::now();
        self.store.update_job(&job).await?;

        Ok(job)
    }

    async fn get_job_history(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<JobExecution>, SchedulerError> {
        self.store.get_job_executions(job_id, limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TriggerType;

    #[test]
    fn test_calculate_next_run_cron() {
        let trigger = TriggerType::Cron {
            expression: "0 0 * * *".to_string(),
            timezone: "UTC".to_string(),
        };
        let job = ScheduledJob::new("test", "hand1", trigger);

        let next = SchedulerEngine::calculate_next_run(&job).unwrap();
        assert!(next.is_some());
    }

    #[test]
    fn test_calculate_next_run_interval() {
        let trigger = TriggerType::Interval {
            seconds: 3600,
            jitter_seconds: None,
        };
        let job = ScheduledJob::new("test", "hand1", trigger);

        let next = SchedulerEngine::calculate_next_run(&job).unwrap();
        assert!(next.is_some());
    }

    #[test]
    fn test_calculate_retry_time() {
        let policy = RetryPolicy {
            max_retries: 3,
            initial_delay_secs: 60,
            backoff_multiplier: 2.0,
            max_delay_secs: 3600,
        };

        let retry0 = SchedulerEngine::calculate_retry_time(&policy, 0);
        let retry1 = SchedulerEngine::calculate_retry_time(&policy, 1);
        let retry2 = SchedulerEngine::calculate_retry_time(&policy, 2);

        // 验证退避增加
        assert!(retry1 > retry0);
        assert!(retry2 > retry1);
    }
}
