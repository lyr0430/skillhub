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
///
/// Safe to forward verbatim: `io::Error`'s `Display` carries the OS error and
/// nothing else — no path, no URL.
pub fn io_error(action: &str, err: &std::io::Error) -> InstallError {
    InstallError::new("io_error", format!("{action} failed: {err}"))
}

/// Longest chunk of a remote-supplied string we echo back to the UI.
///
/// Everything below reaches the web view as a toast, and both sources are
/// influenced by whatever the registry serves — `reqwest::Error` renders the
/// full request URL (so `registry` carrying `user:token@` puts credentials on
/// screen), and `ZipError::InvalidArchive` renders an entry name from the
/// archive, with no bound on its length. Neither belongs in the UI verbatim.
const REMOTE_DETAIL_LIMIT: usize = 200;

fn truncate_detail(detail: &str) -> String {
    if detail.chars().count() <= REMOTE_DETAIL_LIMIT {
        return detail.to_string();
    }
    let kept: String = detail.chars().take(REMOTE_DETAIL_LIMIT).collect();
    format!("{kept}…（已截断）")
}

/// Reduce a network error to something safe to show.
///
/// `reqwest::Error`'s own `Display` is used only when it cannot contain the
/// request URL; otherwise the status and host are reported instead.
pub fn network_error(err: reqwest::Error) -> InstallError {
    let detail = if err.is_connect() || err.is_timeout() || err.is_request() || err.is_body() {
        // These variants may embed the full URL; report the shape instead.
        let host = err
            .url()
            .map(|url| url.host_str().unwrap_or("").to_string());
        match (host.filter(|host| !host.is_empty()), err.status()) {
            (Some(host), Some(status)) => format!("HTTP {status} — {host}"),
            (Some(host), None) => format!("无法连接 {host}"),
            (None, Some(status)) => format!("HTTP {status}"),
            (None, None) => "网络请求失败".to_string(),
        }
    } else {
        truncate_detail(&err.to_string())
    };
    InstallError::new("network_error", format!("下载技能包失败: {detail}"))
}

pub fn zip_error(err: zip::result::ZipError) -> InstallError {
    InstallError::new(
        "zip_error",
        format!("技能包解压失败: {}", truncate_detail(&err.to_string())),
    )
}

pub fn not_found(error: &str) -> InstallError {
    InstallError::new("not_found", error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_detail_leaves_a_short_detail_intact() {
        let short = "HTTP 404";
        assert_eq!(truncate_detail(short), short);
    }

    /// A `ZipError::InvalidArchive` renders an entry name taken from the
    /// archive, which the registry controls and nothing bounds. It reaches the
    /// UI as a toast, so it must not be forwarded at full length.
    #[test]
    fn truncate_detail_bounds_a_remote_supplied_name() {
        let long = "a".repeat(REMOTE_DETAIL_LIMIT * 3);
        let truncated = truncate_detail(&long);

        assert!(truncated.chars().count() < long.chars().count());
        assert!(
            truncated.ends_with("（已截断）"),
            "the user should be told it was cut: {truncated}"
        );
        assert!(truncated.starts_with(&"a".repeat(REMOTE_DETAIL_LIMIT)));
    }

    /// Slicing by bytes would panic or split a character here; the bound is in
    /// characters because entry names and URLs are not ASCII.
    #[test]
    fn truncate_detail_counts_characters_not_bytes() {
        let cjk = "技能".repeat(REMOTE_DETAIL_LIMIT);
        let truncated = truncate_detail(&cjk);

        assert!(truncated.starts_with("技能技能"));
        assert!(truncated.ends_with("（已截断）"));
    }

    /// `reqwest::Error`'s own `Display` embeds the request URL
    /// (`error sending request for url (http://host/path)`), and that string
    /// reaches the UI as a toast. The shape is reported instead: whatever is
    /// wrong, the host is the actionable part and the path is not.
    ///
    /// Note what is *not* asserted here: reqwest redacts `user:password@` from
    /// the URL it renders, so a credential assertion would pass even against
    /// the unfixed code and prove nothing.
    #[test]
    fn network_error_reports_the_host_without_echoing_the_request_url() {
        let err = reqwest::blocking::Client::builder()
            .build()
            .unwrap()
            .get("http://127.0.0.1:1/nope")
            .send()
            .unwrap_err();

        let message = network_error(err).message;
        assert!(
            message.contains("127.0.0.1"),
            "the host is what makes the error actionable: {message}"
        );
        assert!(
            !message.contains("/nope"),
            "the request URL must not be echoed into the UI: {message}"
        );
    }
}
