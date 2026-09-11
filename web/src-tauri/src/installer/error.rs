use serde::Serialize;

/// A cross-platform, user-facing error surfaced to the web view.
#[derive(Debug, Serialize)]
pub struct InstallError {
    pub code: String,
    pub message: String,
}

impl InstallError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for InstallError {}

/// Map an `io::Error` to the human-readable error contract.
pub fn io_error(action: &str, err: &std::io::Error) -> InstallError {
    InstallError::new("io_error", format!("{action} failed: {err}"))
}

pub fn network_error(err: reqwest::Error) -> InstallError {
    InstallError::new("network_error", format!("下载技能包失败: {err}"))
}

pub fn zip_error(err: zip::result::ZipError) -> InstallError {
    InstallError::new("zip_error", format!("技能包解压失败: {err}"))
}

pub fn not_found(error: &str) -> InstallError {
    InstallError::new("not_found", error)
}
