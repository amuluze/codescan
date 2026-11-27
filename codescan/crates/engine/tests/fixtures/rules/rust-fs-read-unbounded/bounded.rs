fn bounded(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > 1024 * 1024 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "too large"));
    }
    std::fs::read(path)
}
