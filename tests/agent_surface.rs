//! The machine-readable surface: `llms.txt`, the markdown twins, and the feed.
//!
//! These files are the site as a program reads it, and nothing links to them
//! visibly, so a regression would ship silently. Each test here asserts a
//! property the llms.txt proposal or the RSS spec requires, against the
//! committed `dist/`.

use std::path::{Path, PathBuf};

fn dist() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dist")
}

fn read(relative: &str) -> String {
    let path = dist().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
}

/// Every post slug, read from the content tree rather than from a list, so a
/// new post is covered without editing this file.
fn slugs() -> Vec<String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("content/blog");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root).unwrap().flatten() {
        let path = entry.path();
        if path.join("index.mdx").is_file() {
            out.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    out.sort();
    out
}

#[test]
fn llms_txt_follows_the_proposal_structure() {
    let text = read("llms.txt");
    // An H1 with the site's name, then a blockquote summary, in that order.
    assert!(text.starts_with("# "), "llms.txt must open with an H1");
    let h1_end = text.find('\n').unwrap();
    let after = text[h1_end..].trim_start();
    assert!(
        after.starts_with('>'),
        "the H1 must be followed by a blockquote summary"
    );
    // Every file list is a markdown bullet with a link.
    for line in text.lines().filter(|line| line.starts_with("- ")) {
        assert!(
            line.contains("]("),
            "a file-list entry must be a link: {line}"
        );
    }
}

#[test]
fn every_link_in_llms_txt_resolves_to_a_written_file() {
    let text = read("llms.txt");
    let mut checked = 0;
    for line in text.lines().filter(|line| line.starts_with("- ")) {
        let Some(start) = line.find("](") else { continue };
        let rest = &line[start + 2..];
        let Some(end) = rest.find(')') else { continue };
        let url = &rest[..end];
        let Some(path) = url.strip_prefix("https://ethang.dev") else {
            panic!("llms.txt links off-site: {url}");
        };
        let path = path.trim_end_matches('/');
        let target = if path.is_empty() {
            dist().join("index.html")
        } else {
            dist().join(path.trim_start_matches('/'))
        };
        assert!(
            target.is_file() || target.join("index.html").is_file(),
            "llms.txt links {url}, which is not in dist/"
        );
        checked += 1;
    }
    assert!(checked > 40, "only {checked} links checked");
}

#[test]
fn every_post_has_a_markdown_twin_that_llms_txt_lists() {
    let text = read("llms.txt");
    for slug in slugs() {
        let twin = format!("blog/{slug}/index.md");
        assert!(
            dist().join(&twin).is_file(),
            "{twin} was not written"
        );
        assert!(
            text.contains(&format!("/{twin}")),
            "llms.txt does not list /{twin}"
        );
    }
}

#[test]
fn a_markdown_twin_names_its_own_address_and_title() {
    let text = read("blog/sessions-are-agents/index.md");
    assert!(text.starts_with("# "), "a twin must open with its title");
    assert!(
        text.contains("https://ethang.dev/blog/sessions-are-agents/"),
        "a twin must carry the URL it is the markdown of"
    );
    // The body is markdown, not HTML: the components expanded to markdown.
    assert!(!text.contains("<PostImage"), "an MDX component survived");
    assert!(!text.contains("import "), "an import line survived");
}

#[test]
fn every_page_advertises_its_twin_and_the_llms_file() {
    let mut checked = 0;
    for entry in walk(&dist()) {
        if entry.extension().and_then(|ext| ext.to_str()) != Some("html") {
            continue;
        }
        let html = std::fs::read_to_string(&entry).unwrap();
        assert!(
            html.contains("rel=\"describedby\""),
            "{} does not point at llms.txt",
            entry.display()
        );
        // The 404 is not a destination and has no twin.
        if entry.file_name().and_then(|name| name.to_str()) == Some("404.html") {
            continue;
        }
        assert!(
            html.contains("type=\"text/markdown\""),
            "{} does not advertise a markdown twin",
            entry.display()
        );
        checked += 1;
    }
    assert!(checked >= 46, "only {checked} pages checked");
}

#[test]
fn llms_full_inlines_what_llms_txt_only_links() {
    let index = read("llms.txt");
    let full = read("llms-full.txt");
    assert!(
        full.len() > index.len() * 10,
        "llms-full.txt ({} bytes) is not carrying the page bodies (index is {} bytes)",
        full.len(),
        index.len()
    );
    // Every post's prose is in the full file, not merely its title.
    for slug in slugs().iter().take(5) {
        let twin = read(&format!("blog/{slug}/index.md"));
        let body: String = twin
            .lines()
            .find(|line| line.starts_with("The ") || line.starts_with("A "))
            .unwrap_or("")
            .chars()
            .take(40)
            .collect();
        if body.len() > 20 {
            assert!(
                full.contains(&body),
                "llms-full.txt is missing the body of {slug}"
            );
        }
    }
}

#[test]
fn the_feed_is_well_formed_rss_with_one_item_per_post() {
    let feed = read("blog/feed.xml");
    assert!(feed.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(feed.contains("<rss version=\"2.0\""));
    assert!(feed.contains("xmlns:atom=\"http://www.w3.org/2005/Atom\""));
    assert!(
        feed.contains("rel=\"self\""),
        "a feed needs an atom:link to itself"
    );
    let posts = slugs().len();
    assert_eq!(
        feed.matches("<item>").count(),
        posts,
        "the feed must carry one item per post"
    );
    assert_eq!(feed.matches("</item>").count(), posts);
    // RFC 822, which is what RSS 2.0 asks for, not ISO 8601.
    let pub_date = feed
        .split("<pubDate>")
        .nth(1)
        .and_then(|rest| rest.split('<').next())
        .expect("no pubDate");
    assert!(
        pub_date.ends_with("+0000"),
        "pubDate is not RFC 822 with a numeric zone: {pub_date}"
    );
    assert!(
        pub_date.contains(", "),
        "pubDate has no weekday: {pub_date}"
    );
}

#[test]
fn every_blog_page_advertises_the_feed() {
    for relative in ["blog/index.html", "blog/page/2/index.html"] {
        let html = read(relative);
        assert!(
            html.contains("application/rss+xml"),
            "{relative} does not advertise the feed"
        );
    }
    // A post does too, and a non-blog page does not.
    assert!(read(&format!("blog/{}/index.html", slugs()[0])).contains("application/rss+xml"));
    assert!(!read("learn/index.html").contains("application/rss+xml"));
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}
