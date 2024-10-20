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

pub struct StreamShare {
    server_url: String,
    chunk_size: usize,
    client: Client,
}

impl StreamShare {
    pub fn new(server_url: String, chunk_size: usize) -> Self {
        Self {
            server_url: server_url,
            chunk_size: chunk_size,
            client: Client::new(),
        }
    }

    pub async fn upload<F>(
        &self,
        file_path: &str,
        mut callback: F,
    ) -> Result<(String, String), Box<dyn std::error::Error>>
    where
        F: FnMut(u64, u64),
    {
        let path = Path::new(file_path);
        let metadata = fs::metadata(path).await?;
        if !metadata.is_file() {
            return Err(format!("Selected item is not a file: {}", file_path).into());
        }

        let file_size = metadata.len();
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");

        let create_url = format!("https://{}/api/create", self.server_url);

        let res = self
            .client
            .post(&create_url)
            .json(&serde_json::json!({ "name": file_name }))
            .send()
            .await?;

        if !res.status().is_success() {
            return Err(format!("Failed to create upload: {}", res.status()).into());
        }

        let create_response: CreateResponse = res.json().await?;
        let ws_url = format!(
            "wss://{}/api/upload/{}",
            self.server_url, create_response.file_identifier
        );
        let (mut ws_stream, _) = connect_async(ws_url).await?;

        let mut file = File::open(path).await?;
        let mut buffer = vec![0u8; self.chunk_size];
        let mut uploaded: u64 = 0;

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }

            let chunk = &buffer[..n];
            ws_stream.send(Message::Binary(chunk.to_vec())).await?;
            uploaded += n as u64;
            callback(uploaded, file_size);

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

    pub async fn delete(
        &self,
        file_identifier: &str,
        deletion_token: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let delete_url = format!(
            "https://{}/api/delete/{}/{}",
            self.server_url, file_identifier, deletion_token
        );

        let res = self.client.delete(&delete_url).send().await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(format!("Failed to delete file: {}", res.status()).into())
        }
    }
}

impl Default for StreamShare {
    fn default() -> Self {
        Self {
            server_url: "streamshare.wireway.ch".to_string(),
            chunk_size: 1024 * 1024,
            client: Client::new(),
        }
    }
}
