# Streamshare

Upload files to [streamshare](https://streamshare.wireway.ch)

#### Example:

```rust
match upload(&file_path).await {
    Ok((file_identifier, _deletion_token)) => {
        let download_url = format!(
            "https://streamshare.wireway.ch/download/{}",
            file_identifier
        );

        println!("File uploaded successfully");
        println!("Download URL: {}", download_url);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```
