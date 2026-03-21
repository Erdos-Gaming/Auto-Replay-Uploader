use std::sync::{Arc, Mutex};
use std::time::Duration;
use crate::config::Config;

// This points to YOUR server, not Discord directly
// const DISCORD_OAUTH_URL: &str = "http://localhost:8080/auth/discord/start";

pub fn ensure_user_id(config: Arc<Mutex<Config>>, timeout: Duration) -> anyhow::Result<u64> {
    {
        let config_guard = config.lock().unwrap();
        if config_guard.discord_id != 0 {
            log::info!("Loaded stored Discord user ID: {}", config_guard.discord_id);
            return Ok(config_guard.discord_id);
        }
    } // release lock before blocking
    
    let cfg = config.lock().unwrap().clone();
    let oauth_url = format!("{}/auth/discord/start", cfg.upload_url);
    
    log::info!("No stored Discord user ID — starting Discord OAuth flow.");
    let id = run_oauth_flow(oauth_url, timeout)?;

    let mut config_guard = config.lock().unwrap();
    config_guard.discord_id = id;
    config_guard.save()?;
    log::info!("Discord user ID obtained and stored: {}", id);
    Ok(id)
}

fn run_oauth_flow(oauth_url: String, timeout: Duration) -> anyhow::Result<u64> {
    // Start local server BEFORE opening browser
    let server = tiny_http::Server::http("127.0.0.1:8085")
        .map_err(|e| anyhow::anyhow!("Failed to bind 127.0.0.1:8085: {}", e))?;

    open_browser(&oauth_url)?;
    log::info!("Browser opened, waiting for OAuth callback...");

    let request = server
        .recv_timeout(timeout)?
        .ok_or_else(|| anyhow::anyhow!("Timed out waiting for OAuth callback"))?;

    let url = request.url().to_string();

    let id = parse_id_from_url(&url)
        .ok_or_else(|| anyhow::anyhow!("No 'id' param in callback URL: {}", url))?;
    
    let html = "<html><body><h2>Authorized with Erdos! You can close this tab.</h2></body></html>";
    let response = tiny_http::Response::from_string(html)
        .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
    request.respond(response)?;
    Ok(id)
}

fn parse_id_from_url(url: &str) -> Option<u64> {
    let query = url.split('?').nth(1)?;
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix("id=") {
            return val.parse().ok();
        }
    }
    None
}

fn open_browser(url: &str) -> anyhow::Result<()> {
    open::that(url).map_err(|e| anyhow::anyhow!("Failed to open browser: {}", e))
}
