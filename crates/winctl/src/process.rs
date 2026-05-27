use std::path::Path;

pub fn basename(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .map(|v| v.to_string_lossy().to_string())
}
