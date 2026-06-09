use crate::modules::{codex_account, codex_wakeup};
use chrono::{DateTime, Datelike, Local, TimeZone};
use std::collections::HashSet;

fn parse_time_to_minutes(value: &str) -> Option<i32> {
    let parts: Vec<&str> = value.trim().split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let hour: i32 = parts[0].parse().ok()?;
    let minute: i32 = parts[1].parse().ok()?;
    if !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    Some(hour * 60 + minute)
}

fn build_local_datetime(date: chrono::NaiveDate, minutes: i32) -> Option<DateTime<Local>> {
    let hour = (minutes / 60) as u32;
    let minute = (minutes % 60) as u32;
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
        .earliest()
        .or_else(|| {
            Local
                .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
                .latest()
        })
}

fn collect_task_reset_timestamps(task: &codex_wakeup::CodexWakeupTask) -> Vec<i64> {
    if task.account_ids.is_empty() {
        return Vec::new();
    }
    let quota_reset_window = task
        .schedule
        .quota_reset_window
        .as_deref()
        .unwrap_or("either");
    let include_primary = quota_reset_window == "either" || quota_reset_window == "primary_window";
    let include_secondary =
        quota_reset_window == "either" || quota_reset_window == "secondary_window";

    let selected: HashSet<&str> = task.account_ids.iter().map(String::as_str).collect();
    let mut timestamps: Vec<i64> = codex_account::list_accounts()
        .into_iter()
        .filter(|account| selected.contains(account.id.as_str()))
        .flat_map(|account| account.quota.into_iter())
        .flat_map(|quota| {
            let mut values = Vec::new();
            if include_primary {
                values.push(quota.hourly_reset_time);
            }
            if include_secondary {
                values.push(quota.weekly_reset_time);
            }
            values
        })
        .flatten()
        .filter(|ts| *ts > 0)
        .collect();
    timestamps.sort_unstable();
    timestamps.dedup();
    timestamps
}

pub fn current_due_at(task: &codex_wakeup::CodexWakeupTask, now: DateTime<Local>) -> Option<i64> {
    match task.schedule.kind.as_str() {
        "daily" => {
            let minutes = parse_time_to_minutes(task.schedule.daily_time.as_deref()?)?;
            let candidate = build_local_datetime(now.date_naive(), minutes)?.timestamp();
            if candidate <= now.timestamp() && task.last_run_at.unwrap_or(0) < candidate {
                Some(candidate)
            } else {
                None
            }
        }
        "weekly" => {
            let minutes = parse_time_to_minutes(task.schedule.weekly_time.as_deref()?)?;
            let weekday = now.weekday().num_days_from_sunday() as i32;
            if !task.schedule.weekly_days.contains(&weekday) {
                return None;
            }
            let candidate = build_local_datetime(now.date_naive(), minutes)?.timestamp();
            if candidate <= now.timestamp() && task.last_run_at.unwrap_or(0) < candidate {
                Some(candidate)
            } else {
                None
            }
        }
        "interval" => {
            let interval_seconds =
                i64::from(task.schedule.interval_hours.unwrap_or(4).max(1)) * 3600;
            let due_at = task.last_run_at.unwrap_or(task.created_at) + interval_seconds;
            if due_at <= now.timestamp() {
                Some(due_at)
            } else {
                None
            }
        }
        "quota_reset" => {
            let last_run_at = task.last_run_at.unwrap_or(task.created_at);
            collect_task_reset_timestamps(task)
                .into_iter()
                .filter(|reset_at| *reset_at <= now.timestamp() && *reset_at > last_run_at)
                .max()
        }
        "startup" => None,
        _ => None,
    }
}

pub fn calculate_next_run_at(task: &codex_wakeup::CodexWakeupTask) -> Option<i64> {
    let now = Local::now();
    match task.schedule.kind.as_str() {
        "daily" => {
            let minutes = parse_time_to_minutes(task.schedule.daily_time.as_deref()?)?;
            for offset in 0..7 {
                let date = now.date_naive() + chrono::Duration::days(offset);
                let candidate = build_local_datetime(date, minutes)?.timestamp();
                if candidate > now.timestamp() {
                    return Some(candidate);
                }
            }
            None
        }
        "weekly" => {
            let minutes = parse_time_to_minutes(task.schedule.weekly_time.as_deref()?)?;
            for offset in 0..14 {
                let date = now.date_naive() + chrono::Duration::days(offset);
                let weekday = date.weekday().num_days_from_sunday() as i32;
                if !task.schedule.weekly_days.contains(&weekday) {
                    continue;
                }
                let candidate = build_local_datetime(date, minutes)?.timestamp();
                if candidate > now.timestamp() {
                    return Some(candidate);
                }
            }
            None
        }
        "interval" => {
            let interval_seconds =
                i64::from(task.schedule.interval_hours.unwrap_or(4).max(1)) * 3600;
            Some(task.last_run_at.unwrap_or(task.created_at) + interval_seconds)
        }
        "quota_reset" => collect_task_reset_timestamps(task)
            .into_iter()
            .filter(|reset_at| *reset_at > now.timestamp())
            .min(),
        "startup" => None,
        _ => None,
    }
}
