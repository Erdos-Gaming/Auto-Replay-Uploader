use crate::config::Config;
use std::path::Path;

pub fn upload_file(path: &Path, config: &Config, discord_user_id: u64) -> anyhow::Result<()> {
    
    log::info!("Upload {} → {} as {}", path.display(), config.upload_url, discord_user_id);
    
    // reqwest::blocking::Request::new(
    //     reqwest::Method::POST,
    //     reqwest::Url::parse(&config.upload_url)?
    // );
    
    // Create a blocking reqwest post to the upload_url/{discord_id} where the bytes of the file are sent in the body.
    reqwest::blocking::Client::new()
        .post(&format!("{}/{}", config.upload_url, discord_user_id))
        .body(std::fs::read(path)?)
        .send()?
        .error_for_status()?;
    
    return Ok(());
    // let file_name = path
    //     .file_name()
    //     .map(|n| n.to_string_lossy().to_string())
    //     .unwrap_or_else(|| "upload".to_string());

    // log::info!("Uploading {} → {}", path.display(), config.upload_url);

    // // Read the file bytes
    // let bytes = std::fs::read(path)?;
    // let file_part = reqwest::blocking::multipart::Part::bytes(bytes)
    //     .file_name(file_name)
    //     .mime_str("application/octet-stream")?;

    // let form = reqwest::blocking::multipart::Form::new()
    //     // The replay file itself
    //     .part(config.field_name.clone(), file_part)
    //     // Discord user ID so the server knows who uploaded it
    //     .text("user_id", discord_user_id.to_string());

    // let client = reqwest::blocking::Client::builder()
    //     .timeout(std::time::Duration::from_secs(60))
    //     .build()?;

    // let mut req = client.post(&config.upload_url).multipart(form);

    // if let Some(token) = &config.auth_token {
    //     req = req.bearer_auth(token);
    // }

    // let response = req.send()?;
    // let status = response.status();

    // if status.is_success() {
    //     log::info!("Upload succeeded: {}", status);
    //     Ok(())
    // } else {
    //     let body = response.text().unwrap_or_default();
    //     anyhow::bail!("Upload failed: HTTP {} — {}", status, body)
    // }
}
