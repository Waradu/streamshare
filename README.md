# Streamshare

Upload files to [streamshare](https://streamshare.wireway.ch)

#### Example:

Upload:
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

Delete:
```rust
match streamshare::delete(file_identifier deletion_token).await {
    Ok(_) => println!("File deleted successfully"),
    Err(e) => eprintln!("Error deleting file: {}", e),
}
```
