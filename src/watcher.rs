use crossbeam_channel::Sender;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;

// const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub fn start_watcher(config: Arc<Mutex<Config>>, dirs: &[String], tx: Sender<PathBuf>) -> anyhow::Result<()> {
    let poll_interval = {
        let cfg = config.lock().unwrap();
        Duration::from_millis(cfg.poll_interval)
    };
    
    let valid_dirs: Vec<PathBuf> = dirs
        .iter()
        .map(|d| Path::new(d).to_path_buf())
        .filter(|p| {
            if p.exists() {
                true
            } else {
                log::warn!("Watch directory does not exist, skipping: {}", p.display());
                false
            }
        })
        .collect();

    if valid_dirs.is_empty() {
        anyhow::bail!(
            "No watch directories could be registered. \
             Check that the paths in config.toml exist."
        );
    }

    thread::spawn(move || {
        let mut seen: HashSet<PathBuf> = valid_dirs
            .iter()
            .flat_map(|d| scan_dir(d))
            .collect();

        log::info!("Watcher seeded with {} existing files", seen.len());

        loop {
            thread::sleep(poll_interval);

            for dir in &valid_dirs {
                for path in scan_dir(dir) {
                    if seen.insert(path.clone()) {
                        
                        log::info!("New file detected: {}", path.display());
                        if let Err(e) = tx.send(path) {
                            log::error!("Channel send error: {}", e);
                            return;
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                path.is_file().then_some(path)
            })
            .collect(),
        Err(e) => {
            log::error!("Scan failed for '{}': {}", dir.display(), e);
            vec![]
        }
    }
}
