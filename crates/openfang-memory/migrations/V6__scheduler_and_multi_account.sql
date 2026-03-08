-- 定时任务表
CREATE TABLE scheduled_jobs (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    hand_id TEXT NOT NULL,
    cron_expression TEXT,
    timezone TEXT DEFAULT 'UTC',
    next_run_at INTEGER,
    last_run_at INTEGER,
    status TEXT DEFAULT 'active',
    retry_policy TEXT, -- JSON
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_scheduled_jobs_status ON scheduled_jobs(status);
CREATE INDEX idx_scheduled_jobs_next_run ON scheduled_jobs(next_run_at);

-- 任务执行历史
CREATE TABLE job_executions (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES scheduled_jobs(id),
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL,
    output TEXT,
    error_message TEXT,
    fuel_consumed INTEGER
);

CREATE INDEX idx_job_executions_job_id ON job_executions(job_id);
CREATE INDEX idx_job_executions_started_at ON job_executions(started_at);

-- 命名空间表 (多账户基础)
CREATE TABLE namespaces (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    config_json TEXT NOT NULL,
    quota_json TEXT,
    created_at INTEGER NOT NULL
);

-- 插入默认命名空间
INSERT INTO namespaces (id, display_name, config_json, created_at)
VALUES ('default', 'Default Namespace', '{}', unixepoch());

-- Agent 命名空间关联
ALTER TABLE agents ADD COLUMN namespace_id TEXT DEFAULT 'default';
CREATE INDEX idx_agents_namespace ON agents(namespace_id);

-- 性能指标表
CREATE TABLE performance_metrics (
    timestamp INTEGER NOT NULL,
    agent_id TEXT,
    hand_name TEXT,
    metric_type TEXT NOT NULL,
    value REAL NOT NULL,
    dimensions TEXT
);

CREATE INDEX idx_metrics_time ON performance_metrics(timestamp);
CREATE INDEX idx_metrics_agent ON performance_metrics(agent_id);
CREATE INDEX idx_metrics_type ON performance_metrics(metric_type);
