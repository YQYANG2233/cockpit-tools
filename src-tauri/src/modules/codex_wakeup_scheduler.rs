use crate::modules::logger;
use chrono::Local;
use cockpit_core::modules::{codex_wakeup, codex_wakeup_scheduler as core_codex_wakeup_scheduler};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;

static STARTED: OnceLock<Mutex<bool>> = OnceLock::new();
static STARTUP_TRIGGERED: OnceLock<Mutex<bool>> = OnceLock::new();

fn started_flag() -> &'static Mutex<bool> {
    STARTED.get_or_init(|| Mutex::new(false))
}

fn startup_triggered_flag() -> &'static Mutex<bool> {
    STARTUP_TRIGGERED.get_or_init(|| Mutex::new(false))
}

fn lock_or_recover<'a, T>(mutex: &'a Mutex<T>, label: &str) -> std::sync::MutexGuard<'a, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(err) => {
            logger::log_warn(&format!("[CodexWakeup] poisoned lock recovered: {label}"));
            err.into_inner()
        }
    }
}

pub fn calculate_next_run_at(task: &codex_wakeup::CodexWakeupTask) -> Option<i64> {
    core_codex_wakeup_scheduler::calculate_next_run_at(task)
}

pub async fn run_task_now(
    app: Option<&AppHandle>,
    task_id: &str,
    trigger_type: &str,
    run_id: Option<String>,
) -> Result<codex_wakeup::CodexWakeupBatchResult, String> {
    let app_for_progress = app.cloned();
    codex_wakeup::run_task_now(task_id, trigger_type, run_id, move |payload| {
        if let Some(app) = app_for_progress.as_ref() {
            let _ = app.emit(codex_wakeup::PROGRESS_EVENT, payload);
        }
    })
    .await
}

pub async fn run_enabled_tasks_now(
    app: Option<&AppHandle>,
    trigger_type: &str,
) -> Result<u32, String> {
    let state = codex_wakeup::load_state_for_scheduler()?;
    if !state.enabled {
        return Ok(0);
    }

    let normalized_trigger = {
        let trimmed = trigger_type.trim();
        if trimmed.is_empty() {
            "startup"
        } else {
            trimmed
        }
    };

    if normalized_trigger == "startup" {
        let app_handle = app.cloned();
        let startup_tasks: Vec<(String, i32)> = state
            .tasks
            .into_iter()
            .filter(|task| task.enabled && task.schedule.kind == "startup")
            .map(|task| {
                (
                    task.id,
                    task.schedule.startup_delay_minutes.unwrap_or(0).max(0),
                )
            })
            .collect();

        for (task_id, delay_minutes) in &startup_tasks {
            let task_id = task_id.clone();
            let app_handle = app_handle.clone();
            let delay_seconds = (*delay_minutes as u64).saturating_mul(60);
            tauri::async_runtime::spawn(async move {
                if delay_seconds > 0 {
                    sleep(Duration::from_secs(delay_seconds)).await;
                }

                let current_state = match codex_wakeup::load_state_for_scheduler() {
                    Ok(state) => state,
                    Err(err) => {
                        logger::log_warn(&format!(
                            "[CodexWakeup] failed to reload startup task state: task_id={task_id}, error={err}"
                        ));
                        return;
                    }
                };
                let should_run = current_state.enabled
                    && current_state.tasks.iter().any(|task| {
                        task.id == task_id && task.enabled && task.schedule.kind == "startup"
                    });
                if !should_run {
                    return;
                }

                if let Err(err) = run_task_now(app_handle.as_ref(), &task_id, "startup", None).await
                {
                    logger::log_warn(&format!(
                        "[CodexWakeup] startup task failed: task_id={task_id}, error={err}"
                    ));
                }
            });
        }
        return Ok(startup_tasks.len() as u32);
    }

    let mut started_count: u32 = 0;
    for task in state.tasks {
        if !task.enabled || task.schedule.kind == "startup" {
            continue;
        }

        match run_task_now(app, &task.id, normalized_trigger, None).await {
            Ok(_) => {
                started_count = started_count.saturating_add(1);
            }
            Err(err) => {
                logger::log_warn(&format!(
                    "[CodexWakeup] enabled task failed: task_id={}, error={err}",
                    task.id
                ));
            }
        }
    }

    Ok(started_count)
}

pub fn trigger_startup_tasks_if_needed(app: AppHandle) {
    let state = match codex_wakeup::load_state_for_scheduler() {
        Ok(state) => state,
        Err(err) => {
            logger::log_warn(&format!(
                "[CodexWakeup] failed to read startup tasks: {err}"
            ));
            return;
        }
    };
    let has_startup_tasks = state
        .tasks
        .iter()
        .any(|task| task.enabled && task.schedule.kind == "startup");
    if !state.enabled || !has_startup_tasks {
        return;
    }

    let should_trigger = {
        let mut startup_triggered = lock_or_recover(
            startup_triggered_flag(),
            "codex wakeup startup trigger lock",
        );
        if *startup_triggered {
            false
        } else {
            *startup_triggered = true;
            true
        }
    };
    if !should_trigger {
        return;
    }

    tauri::async_runtime::spawn(async move {
        match run_enabled_tasks_now(Some(&app), "startup").await {
            Ok(started) => {
                if started > 0 {
                    logger::log_info(&format!(
                        "[CodexWakeup] application startup triggered tasks: started={started}"
                    ));
                }
            }
            Err(err) => {
                logger::log_warn(&format!("[CodexWakeup] startup trigger failed: {err}"));
            }
        }
    });
}

async fn run_scheduler_once(app: &AppHandle) {
    let state = match codex_wakeup::load_state_for_scheduler() {
        Ok(state) => state,
        Err(err) => {
            logger::log_warn(&format!("[CodexWakeup] failed to read task state: {err}"));
            return;
        }
    };

    if !state.enabled {
        return;
    }

    let now = Local::now();
    for task in state.tasks {
        if !task.enabled {
            continue;
        }
        if core_codex_wakeup_scheduler::current_due_at(&task, now).is_none() {
            continue;
        }

        let task_id = task.id.clone();
        let trigger_type = if task.schedule.kind == "quota_reset" {
            "quota_reset"
        } else {
            "scheduled"
        }
        .to_string();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = run_task_now(Some(&app_handle), &task_id, &trigger_type, None).await {
                logger::log_warn(&format!(
                    "[CodexWakeup] scheduled task failed: task_id={task_id}, error={err}"
                ));
            }
        });
    }
}

pub fn ensure_started(app: AppHandle) {
    let mut started = lock_or_recover(started_flag(), "codex wakeup scheduler started lock");
    if *started {
        return;
    }
    *started = true;

    tauri::async_runtime::spawn(async move {
        loop {
            run_scheduler_once(&app).await;
            sleep(Duration::from_secs(30)).await;
        }
    });
}
