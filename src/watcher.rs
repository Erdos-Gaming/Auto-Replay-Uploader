use crossbeam_channel::Sender;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::config::Config;

fn get_autocloud_modified_time(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path.join("steam_autocloud.vdf")).and_then(|m| m.modified()).ok()
}

fn file_age(path: &Path) -> Option<Duration> {
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    SystemTime::now().duration_since(mtime).ok()
}

pub fn start_watcher(config: Arc<Mutex<Config>>, dirs: &[String], tx: Sender<PathBuf>) -> anyhow::Result<()> {
    let poll_interval = {
        let cfg = config.lock().unwrap();
        Duration::from_millis(cfg.poll_interval)
    };
    let max_new_file_age = poll_interval * 3;

    if dirs.is_empty() {
        anyhow::bail!("No watch directories configured.");
    }

    let watch_dirs: Vec<PathBuf> = dirs
        .iter()
        .map(|d| Path::new(d).to_path_buf())
        .collect();

    let mut watch_dirs_autoclouds = HashMap::<PathBuf, SystemTime>::new();

    for dir in &watch_dirs {
        let autocloud = dir.join("steam_autocloud.vdf");
        log::info!("{} steam_autocloud.vdf last modified at {:?}", dir.display(), get_autocloud_modified_time(dir));
        if autocloud.exists() && let Some(modified_time) = get_autocloud_modified_time(&dir) {
            watch_dirs_autoclouds.insert(dir.clone(), modified_time);
        }
    }

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
                let known_vdf = watch_dirs_autoclouds.get(dir).copied();

                let vdf_before = get_autocloud_modified_time(dir);
                let current = scan_dir(dir);
                let vdf_after = get_autocloud_modified_time(dir);

                let vdf_changed = match (known_vdf, vdf_before, vdf_after) {
                    (Some(k), Some(b), _) if b != k => true,
                    (Some(k), _, Some(a)) if a != k => true,
                    (None, Some(_), _) | (None, _, Some(_)) => true,
                    _ => false,
                };

                if vdf_changed {
                    log::info!(
                        "steam_autocloud.vdf changed for {}, absorbing {} files without upload (prev={:?}, before={:?}, after={:?})",
                        dir.display(),
                        current.len(),
                        known_vdf,
                        vdf_before,
                        vdf_after,
                    );
                    for f in current {
                        seen.insert(f);
                    }
                    if let Some(t) = vdf_after.or(vdf_before) {
                        watch_dirs_autoclouds.insert(dir.clone(), t);
                    }
                    continue;
                }

                for path in current {
                    if seen.contains(&path) {
                        continue;
                    }

                    let age = file_age(&path);
                    match age {
                        Some(a) if a <= max_new_file_age => {
                            if seen.insert(path.clone()) {
                                log::info!("Seen set now has {} entries.", seen.len());
                                log::info!("New file detected: {}", path.display());
                                if let Err(e) = tx.send(path) {
                                    log::error!("Channel send error: {:#?}", e);
                                    return;
                                }
                            }
                        }
                        Some(a) => {
                            log::info!(
                                "Ignoring stale file (age {}s, likely cloud-synced): {}",
                                a.as_secs(),
                                path.display()
                            );
                            seen.insert(path);
                        }
                        None => {
                            log::warn!("Ignoring file with unreadable mtime: {}", path.display());
                            seen.insert(path);
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
