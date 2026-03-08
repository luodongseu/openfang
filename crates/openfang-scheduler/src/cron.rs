//! Cron 表达式解析器
//!
//! 支持标准 Unix Cron 格式以及特殊宏

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use std::str::FromStr;

use crate::SchedulerError;

/// Cron 解析器
pub struct CronParser;

impl CronParser {
    /// 解析 Cron 表达式，返回标准格式
    ///
    /// 支持的特殊宏:
    /// - @yearly  / @annually  -> 0 0 1 1 *
    /// - @monthly              -> 0 0 1 * *
    /// - @weekly               -> 0 0 * * 0
    /// - @daily   / @midnight  -> 0 0 * * *
    /// - @hourly               -> 0 * * * *
    pub fn parse(expression: &str) -> Result<String, SchedulerError> {
        let normalized = match expression.trim() {
            "@yearly" | "@annually" => "0 0 1 1 *".to_string(),
            "@monthly" => "0 0 1 * *".to_string(),
            "@weekly" => "0 0 * * 0".to_string(),
            "@daily" | "@midnight" => "0 0 * * *".to_string(),
            "@hourly" => "0 * * * *".to_string(),
            expr => expr.to_string(),
        };

        // 验证表达式有效性
        CronSchedule::from_str(&normalized)
            .map_err(|e| SchedulerError::InvalidCron(e.to_string()))?;

        Ok(normalized)
    }

    /// 获取下一次执行时间
    pub fn next_run(
        expression: &str,
        timezone: &str,
        after: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, SchedulerError> {
        let normalized = Self::parse(expression)?;
        let tz: Tz = timezone.parse().map_err(|_| {
            SchedulerError::InvalidCron(format!("Invalid timezone: {}", timezone))
        })?;

        let schedule = CronSchedule::from_str(&normalized)
            .map_err(|e| SchedulerError::InvalidCron(e.to_string()))?;

        let after_in_tz = after.with_timezone(&tz);

        Ok(schedule
            .after(&after_in_tz)
            .next()
            .map(|dt| dt.with_timezone(&Utc)))
    }

    /// 获取接下来的 N 次执行时间
    pub fn upcoming_runs(
        expression: &str,
        timezone: &str,
        after: DateTime<Utc>,
        count: usize,
    ) -> Result<Vec<DateTime<Utc>>, SchedulerError> {
        let normalized = Self::parse(expression)?;
        let tz: Tz = timezone.parse().map_err(|_| {
            SchedulerError::InvalidCron(format!("Invalid timezone: {}", timezone))
        })?;

        let schedule = CronSchedule::from_str(&normalized)
            .map_err(|e| SchedulerError::InvalidCron(e.to_string()))?;

        let after_in_tz = after.with_timezone(&tz);

        Ok(schedule
            .after(&after_in_tz)
            .take(count)
            .map(|dt| dt.with_timezone(&Utc))
            .collect())
    }

    /// 获取人类可读的描述
    pub fn describe(expression: &str) -> Result<String, SchedulerError> {
        let normalized = Self::parse(expression)?;

        // 简单的描述映射
        let description = match expression.trim() {
            "@yearly" | "@annually" => "每年1月1日午夜执行".to_string(),
            "@monthly" => "每月1日午夜执行".to_string(),
            "@weekly" => "每周日午夜执行".to_string(),
            "@daily" | "@midnight" => "每天午夜执行".to_string(),
            "@hourly" => "每小时执行".to_string(),
            expr => format!("Cron: {}", expr),
        };

        Ok(description)
    }
}

/// 计算间隔触发器的下一次执行
pub fn next_interval_run(
    interval_secs: u64,
    jitter_secs: Option<u64>,
    last_run: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    let base = last_run.unwrap_or_else(Utc::now);
    let next = base + Duration::seconds(interval_secs as i64);

    if let Some(jitter) = jitter_secs {
        use rand::Rng;
        let jitter_amount = rand::thread_rng().gen_range(0..=jitter);
        next + Duration::seconds(jitter_amount as i64)
    } else {
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_special_macros() {
        assert_eq!(CronParser::parse("@yearly").unwrap(), "0 0 1 1 *");
        assert_eq!(CronParser::parse("@annually").unwrap(), "0 0 1 1 *");
        assert_eq!(CronParser::parse("@monthly").unwrap(), "0 0 1 * *");
        assert_eq!(CronParser::parse("@weekly").unwrap(), "0 0 * * 0");
        assert_eq!(CronParser::parse("@daily").unwrap(), "0 0 * * *");
        assert_eq!(CronParser::parse("@midnight").unwrap(), "0 0 * * *");
        assert_eq!(CronParser::parse("@hourly").unwrap(), "0 * * * *");
    }

    #[test]
    fn test_parse_standard_cron() {
        assert!(CronParser::parse("0 0 * * *").is_ok());
        assert!(CronParser::parse("*/5 * * * *").is_ok()); // 步长语法
        assert!(CronParser::parse("0 9-17 * * 1-5").is_ok()); // 范围
        assert!(CronParser::parse("0 0 L * *").is_ok()); // 特殊字符 L
    }

    #[test]
    fn test_parse_invalid_cron() {
        assert!(CronParser::parse("invalid").is_err());
        assert!(CronParser::parse("*").is_err()); // 字段不足
        assert!(CronParser::parse("a b c d e").is_err()); // 无效字符
    }

    #[test]
    fn test_next_run() {
        let now = Utc::now();
        let next = CronParser::next_run("0 0 * * *", "UTC", now).unwrap();
        assert!(next.is_some());
        let next_dt = next.unwrap();
        assert!(next_dt > now);
        assert_eq!(next_dt.hour(), 0);
        assert_eq!(next_dt.minute(), 0);
    }

    #[test]
    fn test_upcoming_runs() {
        let now = Utc::now();
        let runs = CronParser::upcoming_runs("0 0 * * *", "UTC", now, 3).unwrap();
        assert_eq!(runs.len(), 3);
        // 确保时间是递增的
        assert!(runs[1] > runs[0]);
        assert!(runs[2] > runs[1]);
    }

    #[test]
    fn test_describe() {
        assert_eq!(
            CronParser::describe("@daily").unwrap(),
            "每天午夜执行"
        );
        assert_eq!(
            CronParser::describe("0 0 * * *").unwrap(),
            "Cron: 0 0 * * *"
        );
    }
}
