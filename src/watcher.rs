use crossbeam_channel::Sender;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::Config;

pub fn start_watcher(config: Arc<Mutex<Config>>, dirs: &[String], tx: Sender<PathBuf>) -> anyhow::Result<()> {
    let poll_interval = {
        let cfg = config.lock().unwrap();
        Duration::from_millis(cfg.poll_interval)
    };

    if dirs.is_empty() {
        anyhow::bail!("No watch directories configured.");
    }

    let watch_dirs: Vec<PathBuf> = dirs
        .iter()
        .map(|d| Path::new(d).to_path_buf())
        .collect();

    thread::spawn(move || {
        wait_for_dirs(&watch_dirs, poll_interval);

        let mut seen: HashSet<PathBuf> = watch_dirs
            .iter()
            .flat_map(|d| scan_dir(d))
            .collect();

        log::info!("Watcher seeded with {} existing files.", seen.len());

        loop {
            thread::sleep(poll_interval);

            for dir in &watch_dirs {
                for path in scan_dir(dir) {
                    if seen.insert(path.clone()) {
                        log::info!("New file detected: {}", path.display());
                        if let Err(e) = tx.send(path) {
                            log::error!("Channel send error: {:#?}", e);
                            return;
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

fn wait_for_dirs(dirs: &[PathBuf], poll_interval: Duration) {
    let mut logged_waiting = false;

    loop {
        let all_ready = dirs.iter().all(|d| std::fs::read_dir(d).is_ok());

        if all_ready {
            if logged_waiting {
                log::info!("Watch directories are now accessible, seeding...");
            }
            return;
        }

        if !logged_waiting {
            for d in dirs {
                if std::fs::read_dir(d).is_err() {
                    log::info!("Waiting for directory to become accessible: {}", d.display());
                }
            }
            logged_waiting = true;
        }

        thread::sleep(poll_interval);
    }
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
            log::error!("Scan failed for '{}': {:#?}", dir.display(), e);
            vec![]
        }
    }
}