//! File watcher — monitors source files and triggers recompilation/restart.
//!
//! This module provides automatic recompilation and restart capabilities
//! when source files change. It's designed for development workflows where
//! rapid iteration is needed.

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Events emitted by the file watcher.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// Source files changed, recompilation needed.
    SourceChanged(Vec<PathBuf>),
    /// Configuration file changed.
    ConfigChanged(PathBuf),
    /// Agent definition changed.
    AgentChanged(String), // agent name
    /// Skill definition changed.
    SkillChanged(String), // skill name
}

/// File watcher configuration.
#[derive(Debug, Clone)]
pub struct FileWatcherConfig {
    /// Directories to watch for source code changes.
    pub watch_paths: Vec<PathBuf>,
    /// File extensions to monitor.
    pub extensions: Vec<String>,
    /// Debounce duration in milliseconds.
    pub debounce_ms: u64,
    /// Whether to auto-restart on changes.
    pub auto_restart: bool,
    /// Command to run for recompilation (default: "cargo build").
    pub build_command: Option<String>,
    /// Arguments for build command.
    pub build_args: Vec<String>,
    /// Environment variables for build.
    pub build_env: Vec<(String, String)>,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![],
            extensions: vec![
                "rs".to_string(),
                "toml".to_string(),
                "md".to_string(),
            ],
            debounce_ms: 500,
            auto_restart: true,
            build_command: None,
            build_args: vec!["build".to_string()],
            build_env: vec![],
        }
    }
}

/// File watcher handle — dropping this stops the watcher.
pub struct FileWatcherHandle {
    /// Channel to send stop signal.
    _stop_tx: tokio::sync::oneshot::Sender<()>,
    /// Join handle for the watcher task.
    task_handle: tokio::task::JoinHandle<()>,
}

impl FileWatcherHandle {
    /// Stop the file watcher.
    pub async fn stop(self) {
        let _ = self._stop_tx.send(());
        let _ = self.task_handle.await;
    }
}

/// Start watching files for changes.
///
/// Returns a handle that can be used to stop the watcher.
pub async fn start_file_watcher(
    config: FileWatcherConfig,
    event_tx: mpsc::Sender<WatchEvent>,
) -> Result<FileWatcherHandle, Box<dyn std::error::Error + Send + Sync>> {
    let (stop_tx, mut stop_rx) = tokio::sync::oneshot::channel();
    
    let watch_paths = config.watch_paths.clone();
    let extensions = config.extensions.clone();
    let debounce_ms = config.debounce_ms;
    
    // Create async channel for notify events
    let (notify_tx, mut notify_rx) = mpsc::channel::<notify::Result<Event>>(100);
    
    // Create watcher
    let mut watcher = RecommendedWatcher::new(
        move |res: notify::Result<Event>| {
            let _ = notify_tx.blocking_send(res);
        },
        Config::default().with_poll_interval(std::time::Duration::from_millis(debounce_ms)),
    )?;
    
    // Watch specified paths
    for path in &watch_paths {
        if path.exists() {
            match watcher.watch(path, RecursiveMode::Recursive) {
                Ok(_) => info!("Watching path: {}", path.display()),
                Err(e) => warn!("Failed to watch {}: {}", path.display(), e),
            }
        } else {
            warn!("Watch path does not exist: {}", path.display());
        }
    }
    
    // Spawn watcher task
    let task_handle = tokio::spawn(async move {
        let mut debounce_timer: Option<tokio::time::Instant> = None;
        let mut pending_changes: Vec<PathBuf> = vec![];
        
        loop {
            tokio::select! {
                _ = &mut stop_rx => {
                    debug!("File watcher stopping");
                    break;
                }
                
                Some(res) = notify_rx.recv() => {
                    match res {
                        Ok(event) => {
                            // Filter by extension
                            let relevant_paths: Vec<_> = event
                                .paths
                                .into_iter()
                                .filter(|p| {
                                    p.extension()
                                        .and_then(|e| e.to_str())
                                        .map(|e| extensions.iter().any(|ext| ext == e))
                                        .unwrap_or(false)
                                })
                                .collect();
                            
                            if !relevant_paths.is_empty() {
                                debug!("File changes detected: {:?}", relevant_paths);
                                pending_changes.extend(relevant_paths);
                                debounce_timer = Some(tokio::time::Instant::now());
                            }
                        }
                        Err(e) => {
                            warn!("Watch error: {}", e);
                        }
                    }
                }
                
                _ = tokio::time::sleep(std::time::Duration::from_millis(debounce_ms)), if debounce_timer.is_some() => {
                    if let Some(timer) = debounce_timer {
                        if timer.elapsed().as_millis() as u64 >= debounce_ms {
                            // Debounce period passed, process changes
                            if !pending_changes.is_empty() {
                                // Categorize changes
                                let source_changes: Vec<_> = pending_changes
                                    .iter()
                                    .filter(|p| is_source_change(p))
                                    .cloned()
                                    .collect();
                                
                                let config_changes: Vec<_> = pending_changes
                                    .iter()
                                    .filter(|p| is_config_change(p))
                                    .cloned()
                                    .collect();
                                
                                if !source_changes.is_empty() {
                                    let _ = event_tx.send(WatchEvent::SourceChanged(source_changes)).await;
                                }
                                
                                for path in config_changes {
                                    let _ = event_tx.send(WatchEvent::ConfigChanged(path)).await;
                                }
                                
                                pending_changes.clear();
                            }
                            debounce_timer = None;
                        }
                    }
                }
            }
        }
        
        // Drop watcher to stop watching
        drop(watcher);
        info!("File watcher stopped");
    });
    
    Ok(FileWatcherHandle {
        _stop_tx: stop_tx,
        task_handle,
    })
}

/// Check if a path change is a source code change.
fn is_source_change(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e, "rs" | "py" | "js" | "ts" | "go" | "java" | "cpp" | "c" | "h" | "hpp"))
        .unwrap_or(false)
}

/// Check if a path change is a config change.
fn is_config_change(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "toml")
        .unwrap_or(false)
        || path.file_name()
            .and_then(|f| f.to_str())
            .map(|f| f == "config.toml" || f.ends_with(".config.toml"))
            .unwrap_or(false)
}

/// Build the project.
///
/// Runs the build command and returns true if successful.
pub async fn build_project(
    command: Option<&str>,
    args: &[String],
    env: &[(String, String)],
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let cmd = command.unwrap_or("cargo");
    
    info!("Starting build: {} {:?}", cmd, args);
    
    let mut cmd_builder = Command::new(cmd);
    cmd_builder
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    // Set environment variables
    for (key, value) in env {
        cmd_builder.env(key, value);
    }
    
    let output = cmd_builder.output().await?;
    
    if output.status.success() {
        info!("Build successful");
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        error!("Build failed:\n{}", stderr);
        Ok(false)
    }
}

/// Auto-restart configuration for the kernel.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AutoRestartConfig {
    /// Enable auto-restart on source changes.
    pub enabled: bool,
    /// Paths to watch for changes.
    pub watch_paths: Vec<PathBuf>,
    /// File extensions to monitor.
    pub extensions: Vec<String>,
    /// Debounce duration in milliseconds.
    pub debounce_ms: u64,
    /// Build command (default: cargo).
    pub build_command: Option<String>,
    /// Build arguments (default: ["build"]).
    pub build_args: Vec<String>,
    /// Delay before restart after successful build (seconds).
    pub restart_delay_secs: u64,
    /// Maximum number of restart attempts within window.
    pub max_restarts: u32,
    /// Time window for max restarts (seconds).
    pub restart_window_secs: u64,
}

impl Default for AutoRestartConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            watch_paths: vec![],
            extensions: vec![
                "rs".to_string(),
                "toml".to_string(),
            ],
            debounce_ms: 500,
            build_command: None,
            build_args: vec!["build".to_string()],
            restart_delay_secs: 1,
            max_restarts: 5,
            restart_window_secs: 60,
        }
    }
}

/// Restart tracker to prevent restart loops.
pub struct RestartTracker {
    /// Recent restart timestamps.
    restarts: Vec<std::time::Instant>,
    /// Maximum restarts allowed.
    max_restarts: u32,
    /// Time window for counting restarts.
    window: std::time::Duration,
}

impl RestartTracker {
    /// Create a new restart tracker.
    pub fn new(max_restarts: u32, window_secs: u64) -> Self {
        Self {
            restarts: vec![],
            max_restarts,
            window: std::time::Duration::from_secs(window_secs),
        }
    }
    
    /// Record a restart attempt.
    /// Returns false if too many restarts in the window.
    pub fn record_restart(&mut self) -> bool {
        let now = std::time::Instant::now();
        
        // Remove old restarts outside the window
        self.restarts.retain(|t| now.duration_since(*t) < self.window);
        
        // Check if we can restart
        if self.restarts.len() >= self.max_restarts as usize {
            return false;
        }
        
        self.restarts.push(now);
        true
    }
    
    /// Get number of restarts in the current window.
    pub fn restart_count(&self) -> usize {
        let now = std::time::Instant::now();
        self.restarts
            .iter()
            .filter(|t| now.duration_since(**t) < self.window)
            .count()
    }
    
    /// Reset the tracker.
    pub fn reset(&mut self) {
        self.restarts.clear();
    }
}

/// Start the auto-restart system.
///
/// This spawns a background task that watches for file changes,
/// rebuilds the project, and triggers a graceful restart.
pub async fn start_auto_restart(
    config: AutoRestartConfig,
    restart_signal: Arc<tokio::sync::Notify>,
) -> Result<FileWatcherHandle, Box<dyn std::error::Error + Send + Sync>> {
    if !config.enabled {
        return Err("Auto-restart is disabled".into());
    }
    
    let (event_tx, mut event_rx) = mpsc::channel::<WatchEvent>(100);
    
    let watcher_config = FileWatcherConfig {
        watch_paths: config.watch_paths.clone(),
        extensions: config.extensions.clone(),
        debounce_ms: config.debounce_ms,
        auto_restart: true,
        build_command: config.build_command.clone(),
        build_args: config.build_args.clone(),
        build_env: vec![],
    };
    
    let handle = start_file_watcher(watcher_config, event_tx).await?;
    
    let build_command = config.build_command.clone();
    let build_args = config.build_args.clone();
    let restart_delay = std::time::Duration::from_secs(config.restart_delay_secs);
    let mut restart_tracker = RestartTracker::new(config.max_restarts, config.restart_window_secs);
    
    // Spawn the restart coordinator
    tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match event {
                WatchEvent::SourceChanged(paths) => {
                    info!("Source files changed: {:?}", paths);
                    
                    // Check restart rate limit
                    if !restart_tracker.record_restart() {
                        warn!(
                            "Too many restarts ({} in {}s), skipping",
                            restart_tracker.restart_count(),
                            config.restart_window_secs
                        );
                        continue;
                    }
                    
                    // Build the project
                    match build_project(build_command.as_deref(), &build_args, &[]).await {
                        Ok(true) => {
                            // Build successful, wait a bit then restart
                            tokio::time::sleep(restart_delay).await;
                            info!("Triggering graceful restart");
                            restart_signal.notify_one();
                        }
                        Ok(false) => {
                            // Build failed, don't restart
                            warn!("Build failed, not restarting");
                        }
                        Err(e) => {
                            error!("Build error: {}", e);
                        }
                    }
                }
                WatchEvent::ConfigChanged(path) => {
                    info!("Config file changed: {}", path.display());
                    // Config changes are handled separately by the config reload system
                }
                _ => {}
            }
        }
    });
    
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_restart_tracker() {
        let mut tracker = RestartTracker::new(3, 60);
        
        assert!(tracker.record_restart());
        assert!(tracker.record_restart());
        assert!(tracker.record_restart());
        assert!(!tracker.record_restart()); // 4th restart should fail
        
        assert_eq!(tracker.restart_count(), 3);
        
        tracker.reset();
        assert_eq!(tracker.restart_count(), 0);
        assert!(tracker.record_restart());
    }
    
    #[test]
    fn test_is_source_change() {
        assert!(is_source_change(Path::new("src/main.rs")));
        assert!(is_source_change(Path::new("lib.py")));
        assert!(!is_source_change(Path::new("config.toml")));
        assert!(!is_source_change(Path::new("README.md")));
    }
    
    #[test]
    fn test_is_config_change() {
        assert!(is_config_change(Path::new("config.toml")));
        assert!(is_config_change(Path::new("agent.toml")));
        assert!(!is_config_change(Path::new("main.rs")));
    }
}
