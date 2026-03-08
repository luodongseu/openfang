//! 审计日志
//!
//! 记录命名空间级别的操作和事件

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLog {
    pub id: String,
    pub namespace_id: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub actor: Actor,
    pub resource: Resource,
    pub action: String,
    pub result: ActionResult,
    pub metadata: serde_json::Value,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

impl AuditLog {
    pub fn new(
        namespace_id: impl Into<String>,
        event_type: AuditEventType,
        actor: Actor,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            namespace_id: namespace_id.into(),
            timestamp: Utc::now(),
            event_type,
            actor,
            resource: Resource::default(),
            action: String::new(),
            result: ActionResult::Success,
            metadata: serde_json::Value::Null,
            ip_address: None,
            user_agent: None,
        }
    }

    pub fn with_resource(mut self, resource: Resource) -> Self {
        self.resource = resource;
        self
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }

    pub fn with_result(mut self, result: ActionResult) -> Self {
        self.result = result;
        self
    }

    pub fn with_metadata(mut self, metadata: impl Serialize) -> Result<Self, serde_json::Error> {
        self.metadata = serde_json::to_value(metadata)?;
        Ok(self)
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip_address = Some(ip.into());
        self
    }

    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.result = ActionResult::Failure {
            error_code: "GENERIC_ERROR".to_string(),
            error_message: error.into(),
        };
        self
    }
}

/// 审计事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    AgentLifecycle,    // Agent 创建、更新、删除
    HandExecution,     // Hand 执行
    MessageReceived,   // 收到消息
    MessageSent,       // 发送消息
    ConfigChange,      // 配置变更
    AuthAttempt,       // 认证尝试
    PermissionChange,  // 权限变更
    QuotaViolation,    // 配额超限
    SystemEvent,       // 系统事件
    UserAction,        // 用户操作
}

/// 操作者
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Actor {
    User {
        user_id: String,
        username: String,
    },
    Agent {
        agent_id: String,
        agent_name: String,
    },
    System {
        component: String,
    },
    External {
        service: String,
        identifier: String,
    },
}

impl Actor {
    pub fn user(user_id: impl Into<String>, username: impl Into<String>) -> Self {
        Self::User {
            user_id: user_id.into(),
            username: username.into(),
        }
    }

    pub fn agent(agent_id: impl Into<String>, agent_name: impl Into<String>) -> Self {
        Self::Agent {
            agent_id: agent_id.into(),
            agent_name: agent_name.into(),
        }
    }

    pub fn system(component: impl Into<String>) -> Self {
        Self::System {
            component: component.into(),
        }
    }
}

/// 资源
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Resource {
    pub resource_type: String,
    pub resource_id: String,
    pub resource_name: Option<String>,
}

impl Resource {
    pub fn new(resource_type: impl Into<String>, resource_id: impl Into<String>) -> Self {
        Self {
            resource_type: resource_type.into(),
            resource_id: resource_id.into(),
            resource_name: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.resource_name = Some(name.into());
        self
    }
}

/// 操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ActionResult {
    Success,
    Failure {
        error_code: String,
        error_message: String,
    },
    Denied {
        reason: String,
    },
}

/// 审计日志过滤器
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub event_types: Option<Vec<AuditEventType>>,
    pub actor_types: Option<Vec<String>>,
    pub resource_types: Option<Vec<String>>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub only_errors: bool,
}

impl AuditFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_event_types(mut self, types: Vec<AuditEventType>) -> Self {
        self.event_types = Some(types);
        self
    }

    pub fn with_time_range(
        mut self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Self {
        self.start_time = Some(start);
        self.end_time = Some(end);
        self
    }

    pub fn errors_only(mut self) -> Self {
        self.only_errors = true;
        self
    }
}

/// 审计日志存储接口
#[async_trait::async_trait]
pub trait AuditStore: Send + Sync {
    /// 记录审计日志
    async fn log(&self, entry: AuditLog) -> Result<(), crate::NamespaceError>;

    /// 查询审计日志
    async fn query(
        &self,
        namespace_id: &str,
        filter: AuditFilter,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AuditLog>, crate::NamespaceError>;

    /// 获取统计信息
    async fn stats(
        &self,
        namespace_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<AuditStats, crate::NamespaceError>;

    /// 清理过期日志
    async fn purge(
        &self,
        namespace_id: &str,
        before: DateTime<Utc>,
    ) -> Result<u64, crate::NamespaceError>;
}

/// 审计统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStats {
    pub total_events: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub denied_count: u64,
    pub events_by_type: std::collections::HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_creation() {
        let log = AuditLog::new(
            "test_ns",
            AuditEventType::AgentLifecycle,
            Actor::user("U123", "john"),
        )
        .with_resource(Resource::new("agent", "agent_001"))
        .with_action("create")
        .with_metadata(serde_json::json!({ "name": "My Agent" }))
        .unwrap();

        assert_eq!(log.namespace_id, "test_ns");
        assert_eq!(log.action, "create");
        assert_eq!(log.resource.resource_type, "agent");
    }

    #[test]
    fn test_failed_action() {
        let log = AuditLog::new(
            "test_ns",
            AuditEventType::HandExecution,
            Actor::agent("A001", "executor"),
        )
        .failed("Timeout while executing hand");

        match log.result {
            ActionResult::Failure { error_code, error_message } => {
                assert_eq!(error_code, "GENERIC_ERROR");
                assert!(error_message.contains("Timeout"));
            }
            _ => panic!("Expected Failure result"),
        }
    }

    #[test]
    fn test_audit_filter() {
        let filter = AuditFilter::new()
            .with_event_types(vec![AuditEventType::AgentLifecycle, AuditEventType::ConfigChange])
            .errors_only();

        assert!(filter.only_errors);
        assert_eq!(filter.event_types.unwrap().len(), 2);
    }
}
