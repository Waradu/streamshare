use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::{io::Write, path::Path, time::Instant};
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{self, Message},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateResponse {
    file_identifier: String,
    deletion_token: String,
}

pub async fn upload(file_path: &str, show_progress: bool) -> Result<(String, String), Box<dyn std::error::Error>> {
    let path = Path::new(file_path);
    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() {
        return Err("Selected item is not a file".into());
    }

    let file_size = metadata.len();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    let client = Client::new();
    let create_url = "https://streamshare.wireway.ch/api/create";

    let res = client
        .post(create_url)
        .json(&serde_json::json!({ "name": file_name }))
        .send()
        .await?;

    if !res.status().is_success() {
        return Err(format!("Failed to create upload: {}", res.status()).into());
    }

    let create_response: CreateResponse = res.json().await?;
    let ws_url = format!(
        "wss://streamshare.wireway.ch/api/upload/{}",
        create_response.file_identifier
    );
    let (mut ws_stream, _) = connect_async(ws_url).await?;

    let mut file = File::open(path).await?;
    const CHUNK_SIZE: usize = 512 * 1024;
    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut uploaded: u64 = 0;
    let start_time = Instant::now();

    loop {
        let n = file.read(&mut buffer).await?;

        if n == 0 {
            break;
        }

        let chunk = &buffer[..n];
        ws_stream.send(Message::Binary(chunk.to_vec())).await?;
        uploaded += n as u64;

        if show_progress {
            let percentage = (uploaded as f64 / file_size as f64) * 100.0;
            let uploaded_mb = bytes_to_human_readable(uploaded);
            let total_mb = bytes_to_human_readable(file_size);
            let elapsed_secs = start_time.elapsed().as_secs_f64();
            let speed = bytes_to_human_readable((uploaded as f64 / elapsed_secs) as u64);
    
            print!(
                "\r\x1b[2K{:.2}% {}/{} ({}/s)",
                percentage, uploaded_mb, total_mb, speed
            );
            std::io::stdout().flush().unwrap();
        }

        match ws_stream.next().await {
            Some(Ok(Message::Text(text))) if text == "ACK" => (),
            Some(Ok(msg)) => {
                return Err(format!("Unexpected message: {:?}", msg).into());
            }
            Some(Err(e)) => return Err(format!("WebSocket error: {}", e).into()),
            None => return Err("WebSocket closed unexpectedly".into()),
        }
    }

    if show_progress {
        println!(
            "\r\x1b[2K100.00% {}/{} (Upload complete)",
            bytes_to_human_readable(file_size),
            bytes_to_human_readable(file_size)
        );
        println!();
    }

    ws_stream
        .close(Some(tungstenite::protocol::CloseFrame {
            code: tungstenite::protocol::frame::coding::CloseCode::Normal,
            reason: "FILE_UPLOAD_DONE".into(),
        }))
        .await?;

    Ok((
        create_response.file_identifier,
        create_response.deletion_token,
    ))
}

pub async fn delete(
    file_identifier: &str,
    deletion_token: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let delete_url = format!(
        "https://streamshare.wireway.ch/api/delete/{}/{}",
        file_identifier, deletion_token
    );

    let res = client.delete(&delete_url).send().await?;
    if res.status().is_success() {
        Ok(())
    } else {
        Err(format!("Failed to delete file: {}", res.status()).into())
    }
}

fn bytes_to_human_readable(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes as f64 >= GB {
        format!("{:.2}gb", bytes as f64 / GB)
    } else if bytes as f64 >= MB {
        format!("{:.2}mb", bytes as f64 / MB)
    } else if bytes as f64 >= KB {
        format!("{:.2}kb", bytes as f64 / KB)
    } else {
        format!("{}b", bytes)
    }
}
