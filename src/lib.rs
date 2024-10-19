use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
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

pub async fn upload(file_path: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    let path = Path::new(file_path);
    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() {
        return Err("Selected item is not a file".into());
    }

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

    const CHUNK_SIZE: usize = 64 * 1024;

    let mut buffer = vec![0u8; CHUNK_SIZE];

    loop {
        let n = file.read(&mut buffer).await?;

        if n == 0 {
            break;
        }

        let chunk = &buffer[..n];

        ws_stream.send(Message::Binary(chunk.to_vec())).await?;

        match ws_stream.next().await {
            Some(Ok(Message::Text(text))) if text == "ACK" => (),
            Some(Ok(msg)) => {
                return Err(format!("Unexpected message: {:?}", msg).into());
            }
            Some(Err(e)) => return Err(format!("WebSocket error: {}", e).into()),
            None => return Err("WebSocket closed unexpectedly".into()),
        }
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

pub async fn delete(file_identifier: &str, deletion_token: &str) -> Result<(), Box<dyn std::error::Error>> {
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
