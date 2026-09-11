use anyhow::{Context, bail};

use super::{FILE_HOST, MAX_FACE_DESCRIPTORS, validate_response_url};

#[derive(Debug)]
pub(super) struct RemoteFace {
    pub(super) url: reqwest::Url,
    pub(super) style: String,
    pub(super) weight: String,
    pub(super) unicode_range: Option<String>,
}

pub(super) fn parse_css_faces(css: &str, expected_family: &str) -> anyhow::Result<Vec<RemoteFace>> {
    let mut faces = Vec::new();
    for rule in css.split("@font-face").skip(1) {
        let Some(open) = rule.find('{') else { continue };
        let Some(close) = rule[open + 1..].find('}') else {
            bail!("font stylesheet contains an unterminated rule");
        };
        let block = &rule[open + 1..open + 1 + close];
        let family = css_property(block, "font-family")
            .map(unquote)
            .context("font face has no family")?;
        if family != expected_family {
            bail!("font stylesheet returned a different family");
        }
        let style = css_property(block, "font-style").context("font face has no style")?;
        if !matches!(style, "normal" | "italic") {
            bail!("font stylesheet returned an unsupported style");
        }
        let weight = css_property(block, "font-weight").context("font face has no weight")?;
        validate_weight(weight)?;
        let source = css_property(block, "src").context("font face has no source")?;
        if !source.to_ascii_lowercase().contains("format('woff2')")
            && !source.to_ascii_lowercase().contains("format(\"woff2\")")
        {
            bail!("font stylesheet returned a non-WOFF2 face");
        }
        let raw_url = source
            .split_once("url(")
            .and_then(|(_, tail)| tail.split_once(')'))
            .map(|(url, _)| unquote(url.trim()))
            .context("font face has no usable URL")?;
        let url = reqwest::Url::parse(raw_url).context("font face URL is invalid")?;
        validate_response_url(&url, FILE_HOST)?;
        if url.query().is_some() || url.fragment().is_some() || !url.path().ends_with(".woff2") {
            bail!("font face URL is not a canonical WOFF2 asset");
        }
        faces.push(RemoteFace {
            url,
            style: style.to_string(),
            weight: weight.to_string(),
            unicode_range: css_property(block, "unicode-range").map(str::to_string),
        });
        if faces.len() > MAX_FACE_DESCRIPTORS {
            bail!("font stylesheet returned too many faces");
        }
    }
    if faces.is_empty() {
        bail!("font stylesheet contained no usable WOFF2 faces");
    }
    Ok(faces)
}

fn css_property<'a>(block: &'a str, name: &str) -> Option<&'a str> {
    block.split(';').find_map(|declaration| {
        let (key, value) = declaration.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('\'')
        .and_then(|value| value.strip_suffix('\''))
        .or_else(|| {
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
        })
        .unwrap_or(value)
}

pub(super) fn validate_weight(weight: &str) -> anyhow::Result<()> {
    let values = weight
        .split_ascii_whitespace()
        .map(str::parse::<u16>)
        .collect::<Result<Vec<_>, _>>()
        .context("font face weight is invalid")?;
    if values.is_empty()
        || values.len() > 2
        || values.iter().any(|value| !(1..=1000).contains(value))
    {
        bail!("font face weight is out of range");
    }
    Ok(())
}
