//! 路由绑定管理
//!
//! 将消息路由到目标 Agent/Hand/Workflow

use crate::RouteTarget;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 路由绑定定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteBinding {
    pub id: String,
    /// 优先级（数字越小优先级越高）
    pub priority: u32,
    /// 匹配规则
    #[serde(flatten)]
    pub matcher: RouteMatcher,
    /// 路由目标
    pub target: RouteTarget,
    /// 是否启用
    pub enabled: bool,
}

impl RouteBinding {
    pub fn new(priority: u32, matcher: RouteMatcher, target: RouteTarget) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            priority,
            matcher,
            target,
            enabled: true,
        }
    }

    /// 检查是否匹配给定的消息来源
    pub fn matches(
        &self,
        channel: &str,
        account_id: &str,
        peer_id: Option<&str>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        self.matcher.matches(channel, account_id, peer_id)
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// 路由匹配规则
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "match_type", rename_all = "snake_case")]
pub enum RouteMatcher {
    /// 精确匹配
    Exact {
        channel: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        account_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        peer_id: Option<String>,
    },
    /// 模式匹配
    Pattern {
        channel: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        account_pattern: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        peer_pattern: Option<String>,
    },
    /// 通配匹配（所有消息）
    Wildcard {
        #[serde(skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
    },
    /// 多条件匹配（AND）
    All {
        conditions: Vec<RouteMatcher>,
    },
    /// 多条件匹配（OR）
    Any {
        conditions: Vec<RouteMatcher>,
    },
}

impl RouteMatcher {
    /// 创建精确匹配
    pub fn exact(
        channel: impl Into<String>,
        account_id: Option<impl Into<String>>,
        peer_id: Option<impl Into<String>>,
    ) -> Self {
        Self::Exact {
            channel: channel.into(),
            account_id: account_id.map(|s| s.into()),
            peer_id: peer_id.map(|s| s.into()),
        }
    }

    /// 创建渠道通配匹配
    pub fn any() -> Self {
        Self::Wildcard { channel: None }
    }

    /// 创建特定渠道匹配
    pub fn channel(channel: impl Into<String>) -> Self {
        Self::Wildcard {
            channel: Some(channel.into()),
        }
    }

    /// 检查是否匹配
    pub fn matches(
        &self,
        channel: &str,
        account_id: &str,
        peer_id: Option<&str>,
    ) -> bool {
        match self {
            Self::Exact {
                channel: c,
                account_id: a,
                peer_id: p,
            } => {
                if c != channel {
                    return false;
                }
                if let Some(ref acc) = a {
                    if acc != account_id {
                        return false;
                    }
                }
                if let Some(ref pid) = p {
                    if peer_id != Some(pid.as_str()) {
                        return false;
                    }
                }
                true
            }
            Self::Pattern { .. } => {
                // TODO: 实现模式匹配
                false
            }
            Self::Wildcard { channel: c } => match c {
                Some(ref ch) => ch == channel,
                None => true,
            },
            Self::All { conditions } => conditions
                .iter()
                .all(|c| c.matches(channel, account_id, peer_id)),
            Self::Any { conditions } => conditions
                .iter()
                .any(|c| c.matches(channel, account_id, peer_id)),
        }
    }
}

/// 路由结果
#[derive(Debug, Clone)]
pub struct RouteResult {
    pub binding_id: String,
    pub target: RouteTarget,
    pub priority: u32,
}

/// 路由配置 DSL
pub struct RouteBuilder;

impl RouteBuilder {
    /// 构建通用路由（所有渠道的所有消息）
    pub fn catch_all(agent_id: impl Into<String>) -> RouteBinding {
        RouteBinding::new(
            100,
            RouteMatcher::any(),
            RouteTarget::agent(agent_id),
        )
    }

    /// 构建特定渠道路由
    pub fn channel(
        channel: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> RouteBinding {
        RouteBinding::new(
            50,
            RouteMatcher::channel(channel),
            RouteTarget::agent(agent_id),
        )
    }

    /// 构建特定用户路由（高优先级）
    pub fn user(
        channel: impl Into<String>,
        account_id: impl Into<String>,
        peer_id: impl Into<String>,
        agent_id: impl Into<String>,
    ) -> RouteBinding {
        RouteBinding::new(
            1,
            RouteMatcher::exact(channel, Some(account_id), Some(peer_id)),
            RouteTarget::agent(agent_id),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_matcher() {
        let matcher = RouteMatcher::exact(
            "slack",
            Some("work_main"),
            Some("U123"),
        );

        assert!(matcher.matches("slack", "work_main", Some("U123")));
        assert!(!matcher.matches("discord", "work_main", Some("U123")));
        assert!(!matcher.matches("slack", "other_acc", Some("U123")));
        assert!(!matcher.matches("slack", "work_main", Some("U456")));
    }

    #[test]
    fn test_wildcard_matcher() {
        let matcher = RouteMatcher::channel("slack");

        assert!(matcher.matches("slack", "any_acc", None));
        assert!(matcher.matches("slack", "any_acc", Some("any_user")));
        assert!(!matcher.matches("discord", "any_acc", None));

        let any_matcher = RouteMatcher::any();
        assert!(any_matcher.matches("slack", "any_acc", None));
        assert!(any_matcher.matches("discord", "any_acc", None));
    }

    #[test]
    fn test_binding_matching() {
        let binding = RouteBinding::new(
            1,
            RouteMatcher::exact("slack", Some("work_main"), Some("U123")),
            RouteTarget::agent("personal_assistant"),
        );

        assert!(binding.matches("slack", "work_main", Some("U123")));
        assert!(!binding.matches("slack", "work_main", Some("U456")));
    }

    #[test]
    fn test_route_builder() {
        let catch_all = RouteBuilder::catch_all("general_bot");
        assert_eq!(catch_all.priority, 100);

        let channel_route = RouteBuilder::channel("slack", "slack_bot");
        assert_eq!(channel_route.priority, 50);

        let user_route = RouteBuilder::user("slack", "work", "U123", "vip_bot");
        assert_eq!(user_route.priority, 1);
    }

    #[test]
    fn test_disabled_binding() {
        let mut binding = RouteBinding::new(
            1,
            RouteMatcher::any(),
            RouteTarget::agent("test"),
        );
        assert!(binding.matches("slack", "acc", None));

        binding.enabled = false;
        assert!(!binding.matches("slack", "acc", None));
    }
}
