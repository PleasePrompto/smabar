use super::readme::{ReadmeBase, render_readme};

const REPO: &str = "https://github.com/mira-dev/smabar-plugins";
const COMMIT: &str = "9f1c2e7abc";

fn base() -> ReadmeBase {
    ReadmeBase::new(REPO, "mira-dev/smabar-plugins", COMMIT, "plugins/foo")
}

#[test]
fn renders_commonmark_and_the_github_extensions() {
    let html = render_readme(
        "# Hello\n\nSome *text*.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n- [ ] open\n\n~~gone~~\n",
        &base(),
    );
    assert!(html.contains("<h1>Hello</h1>"), "{html}");
    assert!(html.contains("<p>Some <em>text</em>.</p>"), "{html}");
    assert!(
        html.contains("<table>") && html.contains("<td>1</td>"),
        "{html}"
    );
    assert_eq!(html.matches("type=\"checkbox\"").count(), 2, "{html}");
    assert!(html.contains("<del>gone</del>"), "{html}");
}

#[test]
fn gfm_alerts_render_as_blockquotes() {
    let html = render_readme("> [!NOTE]\n> Mind the gap.\n", &base());
    assert!(html.contains("<blockquote"), "{html}");
    assert!(html.contains("Mind the gap."), "{html}");
}

#[test]
fn raw_html_is_dropped_and_its_text_kept() {
    let html = render_readme(
        "<script>alert(1)</script>\n\nplain <b>bold</b> text\n\n<p align=\"center\"><img src=\"logo.png\"></p>\n",
        &base(),
    );
    assert!(!html.contains("<script"), "{html}");
    assert!(!html.contains("alert(1)"), "{html}");
    assert!(!html.contains("<b>"), "{html}");
    assert!(!html.contains("<img"), "{html}");
    assert!(html.contains("plain bold text"), "{html}");
}

#[test]
fn links_are_absolute_and_only_web_schemes_survive() {
    let html = render_readme(
        "[docs](docs/x.md#usage) [home](https://example.org) [mail](mailto:a@b.c) \
         [js](javascript:alert(1)) [top](#top) [up](../secret) [root](/LICENSE) [here](./a.md)",
        &base(),
    );
    assert!(
        html.contains(&format!(
            "href=\"{REPO}/blob/{COMMIT}/plugins/foo/docs/x.md#usage\""
        )),
        "{html}"
    );
    assert!(html.contains("href=\"https://example.org\""), "{html}");
    assert!(html.contains("href=\"mailto:a@b.c\""), "{html}");
    assert!(!html.contains("javascript:"), "{html}");
    assert!(!html.contains("href=\"#top\""), "{html}");
    assert!(!html.contains("secret"), "{html}");
    for text in ["js", "top", "up"] {
        assert!(html.contains(text), "{text} missing: {html}");
    }
    assert!(
        html.contains(&format!("href=\"{REPO}/blob/{COMMIT}/LICENSE\"")),
        "{html}"
    );
    assert!(
        html.contains(&format!("href=\"{REPO}/blob/{COMMIT}/plugins/foo/a.md\"")),
        "{html}"
    );
}

#[test]
fn images_load_only_from_github_hosts() {
    let html = render_readme(
        "![shot](shot.png) ![badge](https://img.shields.io/x.svg) \
         ![raw](https://raw.githubusercontent.com/o/r/c/a.png) \
         ![user](https://user-images.githubusercontent.com/1/2.png) \
         ![up](../x.png) ![plain](http://github.com/o/r/y.png) ![data](data:image/png;base64,AAAA)",
        &base(),
    );
    assert!(
        html.contains(&format!(
            "<img src=\"https://raw.githubusercontent.com/mira-dev/smabar-plugins/{COMMIT}/plugins/foo/shot.png\" alt=\"shot\""
        )),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"https://img.shields.io/x.svg\">badge</a>"),
        "{html}"
    );
    assert!(
        html.contains("<img src=\"https://raw.githubusercontent.com/o/r/c/a.png\""),
        "{html}"
    );
    assert!(
        html.contains("<img src=\"https://user-images.githubusercontent.com/1/2.png\""),
        "{html}"
    );
    assert!(
        html.contains("<a href=\"http://github.com/o/r/y.png\">plain</a>"),
        "{html}"
    );
    assert_eq!(html.matches("<img").count(), 3, "{html}");
    assert!(!html.contains("x.png") && !html.contains("data:"), "{html}");
    for text in ["up", "data"] {
        assert!(html.contains(text), "{text} missing: {html}");
    }
}

#[test]
fn a_foreign_image_inside_a_link_becomes_the_link_text() {
    let html = render_readme(
        "[![badge](https://img.shields.io/b.svg)](https://example.org)",
        &base(),
    );
    assert!(
        html.contains("<a href=\"https://example.org\">badge</a>"),
        "{html}"
    );
    assert!(!html.contains("shields.io"), "{html}");
}

#[test]
fn entry_paths_resolve_to_the_readme_folder() {
    let root = ReadmeBase::new("https://github.com/o/r/", "o/r", "c", ".");
    let plugin = ReadmeBase::new("https://github.com/o/r", "o/r", "c", "plugins/foo");
    let theme = ReadmeBase::new("https://github.com/o/r", "o/r", "c", "themes/night.json");
    let root_theme = ReadmeBase::new("https://github.com/o/r", "o/r", "c", "night.json");
    assert!(
        render_readme("![a](a.png)", &root)
            .contains("src=\"https://raw.githubusercontent.com/o/r/c/a.png\"")
    );
    assert!(
        render_readme("[a](a.md)", &plugin)
            .contains("href=\"https://github.com/o/r/blob/c/plugins/foo/a.md\"")
    );
    assert!(
        render_readme("[a](a.md)", &theme)
            .contains("href=\"https://github.com/o/r/blob/c/themes/a.md\"")
    );
    assert!(
        render_readme("[a](a.md)", &root_theme)
            .contains("href=\"https://github.com/o/r/blob/c/a.md\"")
    );
}
