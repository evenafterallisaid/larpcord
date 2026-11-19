use std::{path::PathBuf, sync::Arc, time::SystemTime};

pub const MAX_FILES: usize = 10;

#[derive(Clone)]
pub struct DraftFile {
    pub name: String,
    pub size: u64,
    pub source: Source,
}

#[derive(Clone)]
pub enum Source {
    Path {
        path: PathBuf,
        modified: Option<SystemTime>,
    },
    Clipboard(Arc<gpui::Image>),
}

impl DraftFile {
    pub fn from_path(path: PathBuf) -> Result<Self, String> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| {
                !name.is_empty() && name.len() <= 255 && !name.chars().any(char::is_control)
            })
            .ok_or("This file name is not supported.")?
            .to_string();
        let metadata = std::fs::metadata(&path).map_err(|_| format!("Could not read {name}."))?;
        if !metadata.is_file() {
            return Err(format!("{name} is not a file."));
        }
        Ok(Self {
            name,
            size: metadata.len(),
            source: Source::Path {
                path,
                modified: metadata.modified().ok(),
            },
        })
    }

    pub fn from_clipboard(image: Arc<gpui::Image>) -> Self {
        let extension = match image.format {
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Svg => "svg",
            format => format.mime_type().strip_prefix("image/").unwrap_or("png"),
        };
        Self {
            name: format!("Pasted image.{extension}"),
            size: image.bytes.len() as u64,
            source: Source::Clipboard(image),
        }
    }

    pub async fn upload_body(&self) -> Result<reqwest::Body, String> {
        use tokio::io::AsyncReadExt as _;
        match &self.source {
            Source::Path { path, modified } => {
                let changed = || {
                    format!(
                        "{} changed or moved. Remove it and attach it again.",
                        self.name
                    )
                };
                let metadata = tokio::fs::metadata(path).await.map_err(|_| changed())?;
                if !metadata.is_file()
                    || metadata.len() != self.size
                    || metadata.modified().ok() != *modified
                {
                    return Err(changed());
                }
                let file = tokio::fs::File::open(path).await.map_err(|_| changed())?;
                let metadata = file.metadata().await.map_err(|_| changed())?;
                if !metadata.is_file()
                    || metadata.len() != self.size
                    || metadata.modified().ok() != *modified
                {
                    return Err(changed());
                }
                Ok(reqwest::Body::wrap_stream(
                    tokio_util::io::ReaderStream::new(file.take(self.size)),
                ))
            }
            Source::Clipboard(image) => Ok(reqwest::Body::from(image.bytes.clone())),
        }
    }
}

pub fn upload_url(raw: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(raw).ok()?;
    (raw.len() <= 4096
        && url.scheme() == "https"
        && url.host_str() == Some("discord-attachments-uploads-prd.storage.googleapis.com")
        && url.port_or_known_default() == Some(443)
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_targets_are_limited_to_discords_signed_storage() {
        assert!(upload_url("https://discord-attachments-uploads-prd.storage.googleapis.com/object?signature=sample").is_some());
        for url in [
            "http://discord-attachments-uploads-prd.storage.googleapis.com/file",
            "https://discord-attachments-uploads-prd.storage.googleapis.com.invalid/file",
            "https://discord-attachments-uploads-prd.storage.googleapis.com:9000/file",
            "https://user@discord-attachments-uploads-prd.storage.googleapis.com/file",
        ] {
            assert!(upload_url(url).is_none());
        }
    }

    #[tokio::test]
    async fn changed_files_never_upload_from_a_stale_draft() {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"first selection").unwrap();
        let draft = DraftFile::from_path(file.path().to_path_buf()).unwrap();
        assert!(draft.upload_body().await.is_ok());
        std::fs::write(file.path(), b"changed").unwrap();
        assert!(draft.upload_body().await.is_err());
    }
}
