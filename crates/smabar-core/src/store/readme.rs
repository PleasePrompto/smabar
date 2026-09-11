//! Renders a listing's README into HTML the settings window can show.
//!
//! The store rule: author text never reaches the shell as markup the author
//! wrote — raw HTML is dropped, every link is absolute and opens in the
//! system browser, images load only from GitHub hosts. Everything else is
//! CommonMark plus the GitHub extensions a README typically uses.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use url::{ParseError, Url};

use super::view::StoreEntry;

/// Where relative README links and images point: the listed commit of the
/// repository, narrowed to the entry's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReadmeBase {
    /// `https://github.com/<owner>/<repo>`, no trailing slash.
    repo_url: String,
    /// `<owner>/<repo>`, as raw.githubusercontent.com addresses it.
    name_with_owner: String,
    commit: String,
    /// The entry's folder inside the repository: empty or ending with `/`.
    dir: String,
}

impl ReadmeBase {
    /// `path` is the listed entry path: `.`, `plugins/<id>` or
    /// `themes/<name>.json`.
    pub(super) fn new(repo_url: &str, name_with_owner: &str, commit: &str, path: &str) -> Self {
        Self {
            repo_url: repo_url.trim_end_matches('/').to_string(),
            name_with_owner: name_with_owner.trim_matches('/').to_string(),
            commit: commit.to_string(),
            dir: entry_dir(path),
        }
    }

    pub(super) fn for_entry(entry: &StoreEntry) -> Self {
        Self::new(
            &entry.repo.url,
            &entry.repo.name_with_owner,
            &entry.commit,
            &entry.path,
        )
    }

    /// A repository path for a relative target: root-relative ones ignore the
    /// entry folder, the rest live beside the README.
    fn resolve(&self, target: Relative<'_>) -> String {
        if target.root {
            target.path.to_string()
        } else {
            format!("{}{}", self.dir, target.path)
        }
    }
}

/// The folder the README lives in, empty or ending with `/`. A theme is a
/// single file, so its README sits beside it in the parent folder.
fn entry_dir(path: &str) -> String {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() || trimmed == "." {
        return String::new();
    }
    let folder = match trimmed.rsplit_once('/') {
        Some((parent, file)) if file.ends_with(".json") => parent,
        None if trimmed.ends_with(".json") => "",
        _ => trimmed,
    };
    if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    }
}

/// A target inside the repository, as written in the README.
#[derive(Debug, Clone, Copy)]
struct Relative<'a> {
    /// Written with a leading `/`: relative to the repository root.
    root: bool,
    path: &'a str,
}

enum Target<'a> {
    Absolute(Url),
    Relative(Relative<'a>),
    /// Anchors, protocol-relative URLs, paths leaving the repository, junk.
    Refused,
}

fn classify(dest: &str) -> Target<'_> {
    if dest.is_empty() || dest.starts_with('#') || dest.starts_with("//") {
        return Target::Refused;
    }
    match Url::parse(dest) {
        Ok(url) => Target::Absolute(url),
        Err(ParseError::RelativeUrlWithoutBase) => {
            let root = dest.starts_with('/');
            let path = dest.trim_start_matches('/');
            let path = path.strip_prefix("./").unwrap_or(path);
            let file = path.split(['?', '#']).next().unwrap_or(path);
            if file.is_empty() || file.split('/').any(|segment| segment == "..") {
                return Target::Refused;
            }
            Target::Relative(Relative { root, path })
        }
        Err(_) => Target::Refused,
    }
}

/// The absolute target of a README link, or `None` for one the shell must
/// not follow: other schemes, in-page anchors and paths leaving the repository.
fn link_target(dest: &str, base: &ReadmeBase) -> Option<String> {
    match classify(dest) {
        Target::Absolute(url) => {
            matches!(url.scheme(), "http" | "https" | "mailto").then(|| dest.to_string())
        }
        Target::Relative(target) => Some(format!(
            "{}/blob/{}/{}",
            base.repo_url,
            base.commit,
            base.resolve(target)
        )),
        Target::Refused => None,
    }
}

enum ImageTarget {
    /// Loaded as an image.
    Image(String),
    /// Shown as a link with the alt text: a host the window must not load from.
    Link(String),
    /// Only the alt text remains.
    Text,
}

fn image_target(dest: &str, base: &ReadmeBase) -> ImageTarget {
    match classify(dest) {
        Target::Absolute(url) => {
            if url.scheme() == "https" && github_host(url.host_str()) {
                ImageTarget::Image(dest.to_string())
            } else if matches!(url.scheme(), "http" | "https") {
                ImageTarget::Link(dest.to_string())
            } else {
                ImageTarget::Text
            }
        }
        Target::Relative(target) => ImageTarget::Image(format!(
            "https://raw.githubusercontent.com/{}/{}/{}",
            base.name_with_owner,
            base.commit,
            base.resolve(target)
        )),
        Target::Refused => ImageTarget::Text,
    }
}

fn github_host(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == "github.com"
        || host == "raw.githubusercontent.com"
        || host.ends_with(".githubusercontent.com")
}

/// What a link or image start became, so its end is treated alike.
enum Open {
    Link,
    Image,
    Dropped,
}

/// Markdown to HTML. Raw HTML (blocks and inline) is dropped, links are
/// rewritten through [`link_target`], images through [`image_target`]; a
/// dropped link or image leaves its text in place.
pub(super) fn render_readme(markdown: &str, base: &ReadmeBase) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM;
    let mut events = Vec::new();
    let mut open = Vec::new();
    let mut links_open = 0usize;
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => match link_target(&dest_url, base) {
                Some(dest_url) => {
                    open.push(Open::Link);
                    links_open += 1;
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url: dest_url.into(),
                        title,
                        id,
                    }));
                }
                None => open.push(Open::Dropped),
            },
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => match image_target(&dest_url, base) {
                ImageTarget::Image(dest_url) => {
                    open.push(Open::Image);
                    events.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url: dest_url.into(),
                        title,
                        id,
                    }));
                }
                // A foreign image becomes a link to it — unless it already sits
                // inside a link, where a nested anchor is not HTML.
                ImageTarget::Link(dest_url) if links_open == 0 => {
                    open.push(Open::Link);
                    links_open += 1;
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url: dest_url.into(),
                        title,
                        id,
                    }));
                }
                ImageTarget::Link(_) | ImageTarget::Text => open.push(Open::Dropped),
            },
            Event::End(TagEnd::Link | TagEnd::Image) => match open.pop() {
                Some(Open::Link) => {
                    links_open -= 1;
                    events.push(Event::End(TagEnd::Link));
                }
                Some(Open::Image) => events.push(Event::End(TagEnd::Image)),
                Some(Open::Dropped) | None => {}
            },
            other => events.push(other),
        }
    }
    let mut rendered = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut rendered, events.into_iter());
    rendered
}
