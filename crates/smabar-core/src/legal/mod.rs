//! The bundled legal texts and the user's acceptance of the terms of use.
//!
//! `legal/terms.{de,en}.md` and `legal/privacy.{de,en}.md` at the repository
//! root are the one source (the website shows copies); `LICENSE` beside them
//! is the license. The terms' frontmatter `updated` date is the version the
//! user accepts: a newer date reopens the notice at the next start. The
//! privacy notice's date is shown only, never asked for.

use std::fs;
use std::io;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::config::SmabarPaths;
use crate::util::{now_ms, write_atomically};

mod render;

const TERMS_DE: &str = include_str!("../../../../legal/terms.de.md");
const TERMS_EN: &str = include_str!("../../../../legal/terms.en.md");
const PRIVACY_DE: &str = include_str!("../../../../legal/privacy.de.md");
const PRIVACY_EN: &str = include_str!("../../../../legal/privacy.en.md");
const LICENSE_MD: &str = include_str!("../../../../LICENSE");
const LICENSE_TITLE: &str = "PolyForm Shield 1.0.0";
const ACCEPTANCE_SCHEMA: u32 = 1;

fn acceptance_schema() -> u32 {
    ACCEPTANCE_SCHEMA
}

/// One legal text, rendered for the settings window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub title: String,
    /// The frontmatter `updated` date (`YYYY-MM-DD`); the license has none.
    pub updated: Option<String>,
    /// Raw HTML dropped, links only to `https:` targets, images as alt text.
    pub html: String,
}

/// The texts in one language plus whether the bar is gated.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalStatus {
    /// The bundled terms are not accepted; the bar shows only the legal tile.
    pub required: bool,
    /// The `updated` date of the bundled terms; empty only in a broken build.
    pub terms_version: String,
    pub privacy_version: String,
    /// Unix milliseconds of the acceptance on file, whichever version it names.
    pub accepted_at: Option<u64>,
    pub terms: Document,
    pub privacy: Document,
    pub license: Document,
}

/// `legal.json`: the terms version the user accepted, and when.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Acceptance {
    #[serde(default = "acceptance_schema")]
    schema: u32,
    terms_version: String,
    accepted_at: u64,
}

struct Localized {
    de: Document,
    en: Document,
}

impl Localized {
    fn parse(what: &'static str, de: &str, en: &str) -> Self {
        Self {
            de: bundled(what, "de", de),
            en: bundled(what, "en", en),
        }
    }

    /// `de` is bundled; every other language reads English, as
    /// `i18n::resolve` does.
    fn pick(&self, language: &str) -> &Document {
        if language == "de" { &self.de } else { &self.en }
    }
}

static TERMS: LazyLock<Localized> = LazyLock::new(|| Localized::parse("terms", TERMS_DE, TERMS_EN));
static PRIVACY: LazyLock<Localized> =
    LazyLock::new(|| Localized::parse("privacy", PRIVACY_DE, PRIVACY_EN));
static LICENSE: LazyLock<Document> = LazyLock::new(|| Document {
    title: LICENSE_TITLE.to_string(),
    updated: None,
    html: render::render(LICENSE_MD),
});

#[derive(Debug, thiserror::Error)]
enum ParseError {
    #[error("the file does not start with a `---` frontmatter block")]
    NoFrontmatter,
    #[error("the frontmatter block is not closed by a `---` line")]
    Unterminated,
    #[error("the frontmatter has no `title`")]
    NoTitle,
    #[error("the frontmatter `updated` is missing or not a YYYY-MM-DD date")]
    NoDate,
}

/// Parses a bundled text. A broken one logs and yields an empty document
/// without a version, which keeps the notice required (fail-safe).
fn bundled(what: &'static str, language: &'static str, source: &str) -> Document {
    parse_document(source).unwrap_or_else(|error| {
        // Unreachable in a healthy build: a test asserts every bundled text parses.
        tracing::error!(what, language, %error, "bundled legal text is invalid; the notice shows an empty page");
        Document {
            title: String::new(),
            updated: None,
            html: String::new(),
        }
    })
}

/// Splits the leading `---` block into `title`/`updated` and renders the
/// body. Only `key: value` lines are read; `\r\n` endings are tolerated.
fn parse_document(source: &str) -> Result<Document, ParseError> {
    let mut lines = source.split_inclusive('\n');
    let fence = lines.next().ok_or(ParseError::NoFrontmatter)?;
    if fence.trim_end() != "---" {
        return Err(ParseError::NoFrontmatter);
    }
    let mut consumed = fence.len();
    let mut title = None;
    let mut updated = None;
    for line in lines {
        consumed += line.len();
        let line = line.trim_end();
        if line == "---" {
            let title = title
                .filter(|title: &&str| !title.is_empty())
                .ok_or(ParseError::NoTitle)?;
            let updated = updated
                .filter(|updated: &&str| is_date(updated))
                .ok_or(ParseError::NoDate)?;
            return Ok(Document {
                title: title.to_string(),
                updated: Some(updated.to_string()),
                html: render::render(&source[consumed..]),
            });
        }
        if let Some((key, value)) = line.split_once(':') {
            match key.trim() {
                "title" => title = Some(value.trim()),
                "updated" => updated = Some(value.trim()),
                _ => {}
            }
        }
    }
    Err(ParseError::Unterminated)
}

/// `YYYY-MM-DD`, so two versions compare correctly as strings.
fn is_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// The version the bundled terms carry; `None` only in a broken build.
fn current_version() -> Option<&'static str> {
    TERMS.en.updated.as_deref()
}

/// Whether `accepted` covers the bundled terms. `>=` keeps a downgrade of
/// smabar accepted; only newer terms reopen the notice.
fn covers_current(accepted: Option<&Acceptance>) -> bool {
    matches!(
        (accepted, current_version()),
        (Some(accepted), Some(current)) if accepted.terms_version.as_str() >= current
    )
}

/// The acceptance on file. A missing file is silent; an unreadable or
/// malformed one warns and counts as no acceptance.
fn load(paths: &SmabarPaths) -> Option<Acceptance> {
    let file = paths.legal_file();
    let raw = match fs::read(&file) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
        Err(error) => {
            tracing::warn!(path = %file.display(), %error, "cannot read the terms acceptance; the notice is shown again");
            return None;
        }
    };
    let parsed = serde_json::from_slice::<Acceptance>(&raw)
        .map_err(io::Error::other)
        .and_then(|acceptance| {
            if is_date(&acceptance.terms_version) {
                Ok(acceptance)
            } else {
                Err(io::Error::other(format!(
                    "termsVersion {:?} is not a YYYY-MM-DD date",
                    acceptance.terms_version
                )))
            }
        });
    match parsed {
        Ok(acceptance) => Some(acceptance),
        Err(error) => {
            tracing::warn!(path = %file.display(), %error, "ignoring an invalid terms acceptance; the notice is shown again");
            None
        }
    }
}

/// Whether the terms bundled in this build are accepted.
pub fn is_accepted(paths: &SmabarPaths) -> bool {
    covers_current(load(paths).as_ref())
}

/// The texts in `language` (`de`, else English) and the acceptance state.
pub fn status(paths: &SmabarPaths, language: &str) -> LegalStatus {
    let accepted = load(paths);
    LegalStatus {
        required: !covers_current(accepted.as_ref()),
        terms_version: current_version().unwrap_or_default().to_string(),
        privacy_version: PRIVACY.en.updated.clone().unwrap_or_default(),
        accepted_at: accepted.map(|acceptance| acceptance.accepted_at),
        terms: TERMS.pick(language).clone(),
        privacy: PRIVACY.pick(language).clone(),
        license: LICENSE.clone(),
    }
}

/// Records the acceptance of the bundled terms version atomically and
/// returns the new status.
pub fn accept(paths: &SmabarPaths, language: &str) -> io::Result<LegalStatus> {
    let terms_version = current_version().ok_or_else(|| {
        io::Error::other(
            "the bundled terms of use carry no `updated` date, so this build cannot record an acceptance; reinstall smabar",
        )
    })?;
    let acceptance = Acceptance {
        schema: ACCEPTANCE_SCHEMA,
        terms_version: terms_version.to_string(),
        accepted_at: now_ms(),
    };
    let mut json = serde_json::to_vec_pretty(&acceptance).map_err(io::Error::other)?;
    json.push(b'\n');
    write_atomically(&paths.legal_file(), &json)?;
    Ok(status(paths, language))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
