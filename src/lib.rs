use futures::{SinkExt, StreamExt};
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::{fs, io::AsyncWriteExt};
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

    pub async fn download(
        &self,
        file_identifier: &str,
        download_path: &str,
        replace: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let res = self
            .client
            .get(format!(
                "https://{}/download/{}",
                self.server_url, file_identifier
            ))
            .send()
            .await?
            .error_for_status()?;

        let unknown = format!("{}.unknown", file_identifier);

        let file_name = res
            .headers()
            .get("content-disposition")
            .and_then(|header| header.to_str().ok())
            .and_then(|header_value| {
                header_value.split(';').find_map(|part| {
                    let trimmed = part.trim();
                    if trimmed.starts_with("filename=") {
                        Some(
                            trimmed
                                .trim_start_matches("filename=")
                                .trim_matches('"')
                                .to_string(),
                        )
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| unknown.clone());

        let expanded_path = shellexpand::tilde(download_path);
        let path = Path::new(&*expanded_path);

        let file_path = if path.as_os_str().is_empty() {
            PathBuf::from(&file_name)
        } else if path.exists() {
            if path.is_dir() {
                path.join(&file_name)
            } else if path.is_file() {
                path.to_path_buf()
            } else {
                return Err(format!(
                    "Path exists but is neither a file nor a directory: {}",
                    path.display()
                )
                .into());
            }
        } else {
            if let Some(parent) = path.parent() {
                if parent.exists() && parent.is_dir() {
                    path.to_path_buf()
                } else {
                    return Err(format!(
                        "Parent directory does not exist or is not a directory: {}",
                        parent.display()
                    )
                    .into());
                }
            } else {
                path.to_path_buf()
            }
        };

        if file_path.exists() && !replace {
            return Err(format!("File already exists: {}", file_path.display()).into());
        }

        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        let mut file = File::create(&file_path).await?;
        let content = res.bytes().await?;

        file.write_all(&content).await?;
        Ok(())
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
