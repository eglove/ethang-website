//! Every link that leaves the site has to open in a new tab.
//!
//! The rule is applied by `src/links.rs` to the finished page, so this test is
//! a check on the built output rather than on any one module: it walks the
//! committed `dist/` and reads every anchor. That is the only place the claim
//! can be made, because the anchors it covers are written in half a dozen
//! modules and by pulldown-cmark, and a new one is added by writing a post.
//!
//! Run `cargo run --release -- build` before `cargo test`: like `parity.rs`
//! and `css_pruning.rs`, this reads `dist/` and not a temporary build.

use std::path::{Path, PathBuf};

fn dist() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dist")
}

fn pages(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            pages(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
            out.push(path);
        }
    }
}

/// The start tags of the anchors in `html`, as written.
fn anchors(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find("<a ") {
        rest = &rest[at..];
        let Some(end) = rest.find('>') else { break };
        out.push(&rest[..end + 1]);
        rest = &rest[end + 1..];
    }
    out
}

/// The value of `name` in one start tag, if it is there.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let at = tag.find(&needle)?;
    let rest = &tag[at + needle.len()..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// Whether `href` names a host other than this site.
fn leaves_the_site(href: &str) -> bool {
    let rest = match href.strip_prefix("//") {
        Some(rest) => rest,
        None => match href.split_once("://") {
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("http") => rest,
            Some((scheme, rest)) if scheme.eq_ignore_ascii_case("https") => rest,
            _ => return false,
        },
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    !host.eq_ignore_ascii_case("ethang.dev") && !host.eq_ignore_ascii_case("www.ethang.dev")
}

#[test]
fn every_external_link_opens_in_a_new_tab() {
    let mut files = Vec::new();
    pages(&dist(), &mut files);
    assert!(files.len() >= 47, "only {} pages found", files.len());

    let mut checked = 0;
    let mut broken = Vec::new();
    for file in &files {
        let html = std::fs::read_to_string(file).unwrap();
        for tag in anchors(&html) {
            let Some(href) = attribute(tag, "href") else {
                continue;
            };
            if !leaves_the_site(href) {
                continue;
            }
            checked += 1;
            if attribute(tag, "target") != Some("_blank") {
                broken.push(format!("{}: {tag}", file.display()));
                continue;
            }
            // A page opened with `target="_blank"` gets a handle on the
            // opener unless the link denies it.
            let rel = attribute(tag, "rel").unwrap_or("");
            for token in ["noopener", "noreferrer"] {
                if !rel.split_whitespace().any(|word| word == token) {
                    broken.push(format!("{}: {tag} has no {token}", file.display()));
                }
            }
        }
    }
    assert!(
        broken.is_empty(),
        "external links that do not open in a new tab:\n{}",
        broken.join("\n")
    );
    assert!(checked > 100, "only {checked} external links checked");
}

#[test]
fn a_link_within_the_site_stays_in_the_tab() {
    let mut files = Vec::new();
    pages(&dist(), &mut files);
    let mut offenders = Vec::new();
    for file in &files {
        let html = std::fs::read_to_string(file).unwrap();
        for tag in anchors(&html) {
            let Some(href) = attribute(tag, "href") else {
                continue;
            };
            if leaves_the_site(href) || href.starts_with("mailto:") || href.starts_with("tel:") {
                continue;
            }
            if attribute(tag, "target").is_some() {
                offenders.push(format!("{}: {tag}", file.display()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "internal links that leave the tab:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn no_anchor_carries_a_duplicated_attribute() {
    let mut files = Vec::new();
    pages(&dist(), &mut files);
    let mut offenders = Vec::new();
    for file in &files {
        let html = std::fs::read_to_string(file).unwrap();
        for tag in anchors(&html) {
            for name in ["target=", "rel=", "href="] {
                if tag.matches(name).count() > 1 {
                    offenders.push(format!("{}: {tag}", file.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "anchors with a duplicated attribute:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn the_markdown_twins_are_untouched() {
    // A twin is markdown, read by a program that does its own linking: the
    // rewrite is an HTML concern and must not have reached these files.
    let twin = dist().join("blog/half-right/index.md");
    let text = std::fs::read_to_string(&twin).unwrap();
    assert!(!text.contains("target=\"_blank\""), "a twin carries HTML");
}