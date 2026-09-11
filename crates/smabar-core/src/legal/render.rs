//! Markdown to HTML for the bundled legal texts.
//!
//! The store's README rule minus the repository-relative targets these texts
//! never use: raw HTML is dropped, a link survives only to an `https:`
//! address (the shell opens it outside), an image leaves its alt text.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use url::Url;

/// What a link or image start became, so its end is treated alike.
enum Open {
    Link,
    Dropped,
}

pub(super) fn render(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM;
    let mut events = Vec::new();
    let mut open = Vec::new();
    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if is_https(&dest_url) {
                    open.push(Open::Link);
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                } else {
                    open.push(Open::Dropped);
                }
            }
            Event::Start(Tag::Image { .. }) => open.push(Open::Dropped),
            Event::End(TagEnd::Link | TagEnd::Image) => match open.pop() {
                Some(Open::Link) => events.push(Event::End(TagEnd::Link)),
                Some(Open::Dropped) | None => {}
            },
            other => events.push(other),
        }
    }
    let mut rendered = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut rendered, events.into_iter());
    rendered
}

fn is_https(dest: &str) -> bool {
    Url::parse(dest).is_ok_and(|url| url.scheme() == "https")
}
