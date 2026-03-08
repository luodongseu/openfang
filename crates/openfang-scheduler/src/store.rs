//! 任务存储接口
//!
//! 定义 JobStore trait 和内存实现（SQLite 实现待后续添加）

use crate::{JobExecution, ScheduledJob, SchedulerError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};

/// 任务存储接口
#[async_trait]
pub trait JobStore: Send + Sync {
    /// 保存任务
    async fn save_job(&self, job: &ScheduledJob) -> Result<(), SchedulerError>;

    /// 更新任务
    async fn update_job(&self, job: &ScheduledJob) -> Result<(), SchedulerError>;

    /// 获取任务
    async fn get_job(&self, id: &str) -> Result<Option<ScheduledJob>, SchedulerError>;

    /// 列出所有任务
    async fn list_jobs(&self) -> Result<Vec<ScheduledJob>, SchedulerError>;

    /// 删除任务
    async fn delete_job(&self, id: &str) -> Result<(), SchedulerError>;

    /// 获取待执行的任务（next_run_at <= now 且 status = active）
    async fn get_pending_jobs(
        &self,
        before: DateTime<Utc>,
    ) -> Result<Vec<ScheduledJob>, SchedulerError>;

    /// 保存执行记录
    async fn save_execution(&self, execution: &JobExecution) -> Result<(), SchedulerError>;

    /// 更新执行记录
    async fn update_execution(
        &self,
        execution: &JobExecution,
    ) -> Result<(), SchedulerError>;

    /// 获取任务的执行历史
    async fn get_job_executions(
        &self,
        job_id: &str,
        limit: usize,
    ) -> Result<Vec<JobExecution>, SchedulerError>;
}

/// 内存存储实现
pub mod memory {
    use super::*;
    use dashmap::DashMap;
    use std::sync::Arc;

    pub struct MemoryJobStore {
        jobs: Arc<DashMap<String, ScheduledJob>>,
        executions: Arc<DashMap<String, JobExecution>>,
    }

    impl MemoryJobStore {
        pub fn new() -> Self {
            Self {
                jobs: Arc::new(DashMap::new()),
                executions: Arc::new(DashMap::new()),
            }
        }
    }

    impl Default for MemoryJobStore {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl JobStore for MemoryJobStore {
        async fn save_job(
            &self,
            job: &ScheduledJob,
        ) -> Result<(), SchedulerError> {
            self.jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn update_job(
            &self,
            job: &ScheduledJob,
        ) -> Result<(), SchedulerError> {
            self.jobs.insert(job.id.clone(), job.clone());
            Ok(())
        }

        async fn get_job(
            &self,
            id: &str,
        ) -> Result<Option<ScheduledJob>, SchedulerError> {
            Ok(self.jobs.get(id).map(|j| j.clone()))
        }

        async fn list_jobs(&self) -> Result<Vec<ScheduledJob>, SchedulerError> {
            Ok(self.jobs.iter().map(|entry| entry.value().clone()).collect())
        }

        async fn delete_job(&self,
            id: &str,
        ) -> Result<(), SchedulerError> {
            self.jobs.remove(id);
            Ok(())
        }

        async fn get_pending_jobs(
            &self,
            before: DateTime<Utc>,
        ) -> Result<Vec<ScheduledJob>, SchedulerError> {
            Ok(self
                .jobs
                .iter()
                .filter(|entry| {
                    let j = entry.value();
                    j.status == crate::JobStatus::Scheduled
                        && j.next_run_at.map_or(false, |t| t <= before)
                })
                .map(|entry| entry.value().clone())
                .collect())
        }

        async fn save_execution(
            &self,
            execution: &JobExecution,
        ) -> Result<(), SchedulerError> {
            self.executions.insert(execution.id.clone(), execution.clone());
            Ok(())
        }

        async fn update_execution(
            &self,
            execution: &JobExecution,
        ) -> Result<(), SchedulerError> {
            self.executions.insert(execution.id.clone(), execution.clone());
            Ok(())
        }

        async fn get_job_executions(
            &self,
            job_id: &str,
            limit: usize,
        ) -> Result<Vec<JobExecution>, SchedulerError> {
            let mut executions: Vec<_> = self
                .executions
                .iter()
                .filter(|entry| entry.value().job_id == job_id)
                .map(|entry| entry.value().clone())
                .collect();

            executions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
            executions.truncate(limit);

            Ok(executions)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScheduledJob, TriggerType};

    #[tokio::test]
    async fn test_memory_store() {
        let store = memory::MemoryJobStore::new();

        // 创建任务
        let trigger = TriggerType::Interval {
            seconds: 3600,
            jitter_seconds: None,
        };
        let job = ScheduledJob::new("test", "hand1", trigger);
        store.save_job(&job).await.unwrap();

        // 获取任务
        let retrieved = store.get_job(&job.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "test");

        // 列出任务
        let jobs = store.list_jobs().await.unwrap();
        assert_eq!(jobs.len(), 1);

        // 删除任务
        store.delete_job(&job.id).await.unwrap();
        let retrieved = store.get_job(&job.id).await.unwrap();
        assert!(retrieved.is_none());
    }
}
