use serde::Serialize;

use crate::installer::error::InstallError;
use crate::installer::metadata::InstalledMetadata;

/// Longest URL we will hand to the operating system. Far beyond any real
/// project link, but bounded so a malformed frontmatter value cannot become a
/// huge argv entry.
const MAX_URL_LEN: usize = 2048;

/// Where a resolved homepage came from, so the UI can label the link honestly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HomepageSource {
    /// Declared by the skill author in `SKILL.md` frontmatter.
    Frontmatter,
    /// Recorded locally in `.skillhub/metadata.json`.
    Metadata,
    /// Derived from the skill's coordinates: the SkillHub skill page.
    Registry,
}

/// Resolve the link a local skill's title should offer, in priority order:
///
/// 1. the author's own `homepage` from `SKILL.md` (the real source repository),
/// 2. the address recorded in `.skillhub/metadata.json` (an out-of-band
///    homepage, for skills whose `SKILL.md` does not carry one),
/// 3. the SkillHub skill page derived from `registry` + coordinates, which is
///    what makes an installed skill linkable at all.
///
/// Candidates that fail [`validate_external_url`] are skipped rather than
/// surfaced, so a hand-edited `SKILL.md` cannot inject a `javascript:` link.
pub fn resolve_homepage(
    frontmatter: Option<String>,
    metadata: Option<&InstalledMetadata>,
) -> (Option<String>, Option<HomepageSource>) {
    if let Some(url) = frontmatter.filter(|url| validate_external_url(url).is_ok()) {
        return (Some(url), Some(HomepageSource::Frontmatter));
    }

    let recorded = metadata
        .and_then(|meta| meta.homepage.as_ref())
        .filter(|url| validate_external_url(url).is_ok())
        .cloned();
    if let Some(url) = recorded {
        return (Some(url), Some(HomepageSource::Metadata));
    }

    match metadata.and_then(skill_page_url) {
        Some(url) => (Some(url), Some(HomepageSource::Registry)),
        None => (None, None),
    }
}

/// Build the SkillHub skill page URL for a managed install.
///
/// Mirrors the web route `/space/$namespace/$slug` (`web/src/app/router.tsx`).
/// Coordinate slugs are restricted to `[A-Za-z0-9._-]` by the registry's own
/// rules, so they need no percent-encoding here.
fn skill_page_url(metadata: &InstalledMetadata) -> Option<String> {
    if metadata.registry.is_empty() || metadata.namespace.is_empty() || metadata.slug.is_empty() {
        return None;
    }
    let base = metadata.registry.trim_end_matches('/');
    let url = format!(
        "{base}/space/{}/{slug}",
        metadata.namespace,
        slug = metadata.slug
    );
    validate_external_url(&url).ok().map(|_| url)
}

/// Validate a URL before handing it to the operating system's default browser.
///
/// The value can originate from a `SKILL.md` we did not author, so the check is
/// an allowlist on the scheme rather than a denylist: anything that could
/// execute script, read local files, or confuse the shell command that opens it
/// is rejected. Whitespace is refused outright because raw spaces would be
/// re-split by any intermediate shell.
pub fn validate_external_url(url: &str) -> Result<(), InstallError> {
    let trimmed = url.trim();

    if trimmed.is_empty() {
        return Err(InstallError::new("invalid_url", "来源地址为空"));
    }
    if trimmed.len() > MAX_URL_LEN {
        return Err(InstallError::new(
            "invalid_url",
            format!("来源地址过长（超过 {MAX_URL_LEN} 字节）"),
        ));
    }
    if trimmed.chars().any(char::is_whitespace) {
        return Err(InstallError::new("invalid_url", "来源地址包含空白字符"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(InstallError::new("invalid_url", "来源地址包含控制字符"));
    }

    let lowered = trimmed.to_ascii_lowercase();
    let scheme_len = if lowered.starts_with("https://") {
        "https://".len()
    } else if lowered.starts_with("http://") {
        "http://".len()
    } else {
        return Err(InstallError::new(
            "invalid_url",
            format!("仅支持 http/https 来源地址，收到: {trimmed}"),
        ));
    };

    // Require an actual host: `http://` alone, or `http:///path`, is not openable.
    let rest = &trimmed[scheme_len..];
    if rest.is_empty() || rest.starts_with('/') || rest.starts_with('?') || rest.starts_with('#') {
        return Err(InstallError::new(
            "invalid_url",
            format!("来源地址缺少主机名: {trimmed}"),
        ));
    }

    Ok(())
}
