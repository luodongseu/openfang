OpenFang Vibe Coding 开发实施计划

  基于 OpenFang vs OpenClaw 深度对比报告，本计划详细规划如何在 OpenFang 基础上实现 OpenClaw
  的关键功能（自优化更新、定时任务、多账户等），同时保持其安全与效率优势。

  项目背景

  - 目标项目: OpenFang - 生产级 Agent 操作系统
  - 技术栈: Rust + Tokio + WASM
  - 当前规模: 14 crates, 137k+ 行代码, 1,767+ 测试用例
  - 核心优势: 40MB 内存占用, <200ms 冷启动, 16 层安全机制

  ---
  实施阶段总览

  ┌─────────────────────────────────────────────────────────────────────────────┐
  │                           Vibe Coding 路线图                                  │
  ├──────────┬──────────┬──────────┬──────────┬──────────┬──────────┬─────────────┤
  │  第1阶段  │  第2阶段  │  第3阶段  │  第4阶段  │  第5阶段  │  第6阶段  │   第7阶段    │
  │ 基础设施  │ 定时任务  │ 多账户   │ 自优化   │ 技能市场 │ 数据迁移 │  生产就绪    │
  │  (2周)   │  (2周)   │  (2周)   │  (3周)   │  (3周)   │  (1周)   │   (2周)      │
  └──────────┴──────────┴──────────┴──────────┴──────────┴──────────┴─────────────┘

  总工期: 约 15 周（可并行优化）

  ---
  第1阶段: 基础设施准备 (2周)

  目标

  搭建开发环境，创建核心扩展模块，建立测试框架。

  任务清单

  1.1 开发环境配置

  - 验证 Rust 1.75+ 和 Cargo 环境
  - 配置 IDE (Rust Analyzer, Clippy, rustfmt)
  - 建立本地测试数据库
  - 设置 CI/CD 流水线模板

  1.2 数据库 Schema 扩展

  文件: crates/openfang-memory/migrations/V6__scheduler_and_multi_account.sql

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

  -- 命名空间表 (多账户基础)
  CREATE TABLE namespaces (
      id TEXT PRIMARY KEY,
      display_name TEXT NOT NULL,
      config_json TEXT NOT NULL,
      quota_json TEXT,
      created_at INTEGER NOT NULL
  );

  -- Agent 命名空间关联
  ALTER TABLE agents ADD COLUMN namespace_id TEXT DEFAULT 'default';

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

  1.3 新 Crate 创建

  cargo new --lib crates/openfang-scheduler
  cargo new --lib crates/openfang-namespace
  cargo new --lib crates/openfang-optimizer

  1.4 核心依赖

  [dependencies]
  tokio-cron-scheduler = "0.13"
  cron = "0.15"
  chrono-tz = "0.10"
  serde_regex = "1.1"
  wit-bindgen = "0.35"
  ed25519-dalek = "2.1"
  statrs = "0.18"

  ---
  第2阶段: 定时任务系统 (2周)

  核心实现

  Cron 解析器特性:
  - 标准 Unix Cron 5 字段格式
  - 特殊宏: @yearly, @monthly, @daily, @hourly
  - IANA 时区数据库，正确处理夏令时
  - 步长语法: */5

  任务状态机:
  pub enum JobStatus {
      Scheduled,   // 已调度，等待执行
      Running,     // 正在执行
      Succeeded,   // 执行成功
      Failed { retry_count: u32 },  // 可重试
      DeadLetter,  // 死信队列
      Paused,      // 已暂停
      Cancelled,   // 已取消
  }

  触发器类型:
  - Cron: 表达式 + 时区
  - Interval: 秒数 + 抖动
  - Event: 模式匹配
  - Condition: 指标阈值

  API 端点:
  - GET /api/scheduler/jobs
  - POST /api/scheduler/jobs
  - POST /api/scheduler/jobs/:id/control (pause/resume/trigger/cancel)
  - GET /api/scheduler/jobs/:id/history

  ---
  第3阶段: 多账户管理系统 (2周)

  核心实现

  命名空间结构:
  - 隔离的 Agent 注册表 (DashMap)
  - 隔离的 SQLite 存储连接
  - 资源配额管理
  - 命名空间级审计日志

  路由规则配置:
  [namespace]
  id = "personal"
  display_name = "Personal Space"

  # 同一渠道的多个账户
  [[namespace.channels.telegram]]
  account_id = "personal_main"
  token = { env = "TELEGRAM_PERSONAL_TOKEN" }

  [[namespace.channels.telegram]]
  account_id = "personal_blog"
  token = { env = "TELEGRAM_BLOG_TOKEN" }

  # 路由规则（最具体优先）
  [[namespace.bindings]]
  priority = 1
  match = { channel = "slack", account_id = "work_main", peer_id = "UCEO123" }
  target = { type = "agent", agent_id = "executive_assistant" }

  [[namespace.bindings]]
  priority = 10
  match = { channel = "slack", account_id = "work_main" }
  target = { type = "agent", agent_id = "general_assistant" }

  ---
  第4阶段: 自优化更新机制 (3周)

  核心实现

  OptimizerHand:
  - 优化周期: 每 6 小时
  - 触发条件: 指标阈值 / 用户反馈 / 成本预警
  - 优化目标: ModelSelection / ScheduleFrequency / ContextCompression

  A/B 测试框架:
  - 实验管理: 创建 / 分配 / 分析 / 回滚
  - 流量分割: 一致性哈希
  - 统计检验: 效应大小 / 置信区间 / p值
  - 自动决策: p < 0.05 且效应 > 0 时采用

  指标类型:
  - Latency / Cost / Quality / Resource / ErrorRate

  ---
  第5阶段: 技能市场与生态 (3周)

  核心实现

  FangHub 架构:
  [开发者] --WIT + WASM--> [构建服务] --签名--> [注册中心]
                                |
                                v
                          [安全扫描] --通过--> [测试沙箱]
                                |              |
                                拒绝            v
                                |         [性能基准]
                                v              |
                          [人工审计] <--复杂-- [自动发布]
                                |
                                v
                          [分级标签: verified/production/experimental]

  WIT 接口定义:
  package openfang:skill@0.2.0;

  interface tool {
      exec: func(input: input) -> output;
  }

  interface config {
      get-schema: func() -> schema;
      validate: func(config: string) -> result<_, validation-error>;
  }

  world skill {
      export tool;
      export config;
      import openfang:system/llm@0.2.0;
      import openfang:system/storage@0.2.0;
  }

  ---
  第6阶段: 数据迁移工具 (1周)

  CLI 命令:
  openfang migrate \
    --source ~/.openclaw \
    --namespace personal \
    --components soul,memory,sessions,cron

  迁移映射:

  ┌──────────────────────┬─────────────────┬──────────────────┐
  │    源 (OpenClaw)     │ 目标 (OpenFang) │       工具       │
  ├──────────────────────┼─────────────────┼──────────────────┤
  │ SOUL.md              │ Agent 配置      │ migrate soul     │
  ├──────────────────────┼─────────────────┼──────────────────┤
  │ MEMORY.md            │ 向量嵌入        │ migrate memory   │
  ├──────────────────────┼─────────────────┼──────────────────┤
  │ memory/YYYY-MM-DD.md │ 会话历史        │ migrate sessions │
  ├──────────────────────┼─────────────────┼──────────────────┤
  │ HEARTBEAT.md         │ 定时任务        │ migrate cron     │
  └──────────────────────┴─────────────────┴──────────────────┘

  ---
  第7阶段: 生产就绪 (2周)

  测试目标

  - 单元测试: 新增 330+，覆盖 >80%
  - 集成测试: 定时任务执行、恢复、夏令时边界
  - 性能基准: 1000 并发任务

  监控指标

  // Prometheus 指标
  openfang_job_executions_total
  openfang_job_latency_seconds
  openfang_active_namespaces

  文档清单

  - API 文档 (OpenAPI/Swagger)
  - 定时任务配置指南
  - 多账户设置教程
  - 技能开发 SDK 文档
  - 迁移指南 (OpenClaw → OpenFang)

  ---
  风险与缓解

  ┌────────────────┬──────┬──────────────────────────────┐
  │      风险      │ 影响 │           缓解措施           │
  ├────────────────┼──────┼──────────────────────────────┤
  │ 夏令时边界错误 │ 高   │ 全面测试，使用 chrono-tz     │
  ├────────────────┼──────┼──────────────────────────────┤
  │ WASM 沙箱逃逸  │ 高   │ 使用 wasmtime，定期安全审计  │
  ├────────────────┼──────┼──────────────────────────────┤
  │ 数据迁移丢失   │ 高   │ 备份策略，增量迁移，验证     │
  ├────────────────┼──────┼──────────────────────────────┤
  │ 性能退化       │ 中   │ 基准测试，性能监控，回滚能力 │
  └────────────────┴──────┴──────────────────────────────┘

  ---
