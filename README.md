# Streamshare (official)

Upload files to [streamshare](https://streamshare.wireway.ch)

#### Example:

Upload:

```rust
let client = StreamShare::default();

let callback = |uploaded_bytes, total_bytes| {
    println!(
        "Uploaded {}b of {}b",
        uploaded_bytes,
        total_bytes
    );
}

match client.upload(&file_path, callback).await {
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
let client = StreamShare::default();

match client.delete(file_identifier, deletion_token).await {
    Ok(_) => println!("File deleted successfully"),
    Err(e) => eprintln!("Error deleting file: {}", e),
}
```

Download:

```rust
let client = StreamShare::default();

match client.download(file_identifier, path).await {
    Ok(_) => println!("File downloaded successfully"),
    Err(e) => eprintln!("Error downloaded file: {}", e),
}
```

Check [toss](https://github.com/Waradu/to-streamshare) for a better example on how to use it.
