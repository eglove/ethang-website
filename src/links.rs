//! External links open in a new tab.
//!
//! A link that leaves the site should not take the reader's place on it: the
//! reader came here to read this page, and a same-tab navigation to somebody
//! else's site loses that place. So every anchor whose `href` names another
//! host gets `target="_blank"`, with `rel="noopener noreferrer"` beside it -
//! `noopener` because a page opened with `target="_blank"` otherwise gets a
//! handle on the opener through `window.opener`, and `noreferrer` because the
//! pair is what the rest of the site already writes by hand.
//!
//! The rewrite runs once, over the finished page, rather than at each of the
//! dozen places an anchor is built: a post's markdown links, the Learn page's
//! entries, the home page's profile row, and a blockquote's source all arrive
//! as HTML from different modules, and a rule that has to be remembered in
//! each of them is a rule that will be forgotten in one.
//! `tests/external_links.rs` checks every page the build writes.
//!
//! What counts as external is the host: `https://ethang.dev/...` is this site
//! and stays in the tab, `mailto:` and `tel:` are not navigations at all, and
//! a relative path or a `#fragment` is this page.

use crate::html::find_ascii_case_insensitive;

/// The host this site is served from, and the `www` alias the deploy config
/// answers on too. A link to either is a link to this site.
const SITE_HOSTS: [&str; 2] = ["ethang.dev", "www.ethang.dev"];

/// Elements whose content is text, not markup. An `<a` inside one is not a
/// link, so the element is copied through untouched.
const RAW_TEXT: [&str; 4] = ["pre", "code", "style", "textarea"];

/// Mark every external link in `html` as opening in a new tab.
pub fn external_links(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 64);
    let mut rest = html;
    loop {
        let Some(at) = rest.find('<') else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        rest = &rest[at..];

        // A comment is copied through: nothing inside it is a tag.
        if rest.starts_with("<!--") {
            let end = rest.find("-->").map(|at| at + 3).unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }

        // A raw-text element's content is text, not markup, so an `<a` in it
        // is not a link. The whole element is copied through.
        if let Some(name) = raw_text_element(rest) {
            let close = format!("</{name}");
            let end = find_ascii_case_insensitive(rest, &close)
                .and_then(|at| rest[at..].find('>').map(|end| at + end + 1))
                .unwrap_or(rest.len());
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }

        let Some(len) = tag_len(rest) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rewrite_tag(&rest[..len]));
        rest = &rest[len..];
    }
}

/// Whether `href` points at a host other than this site.
///
/// A relative path, a `#fragment`, and a `mailto:` or `tel:` URL are all
/// navigations that stay where they are: the first two are this page, and the
/// last two are handed to another program rather than to a tab.
pub fn is_external(href: &str) -> bool {
    let href = href.trim();
    let rest = match href.strip_prefix("//") {
        Some(rest) => rest,
        None => match href.split_once("://") {
            Some((scheme, rest)) => {
                if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
                    return false;
                }
                rest
            }
            None => return false,
        },
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // `user:password@host:port` - the host is the middle of that.
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    !SITE_HOSTS.iter().any(|site| host.eq_ignore_ascii_case(site))
}

/// `tag`, with `target` and `rel` added when it is an anchor to another host.
fn rewrite_tag(tag: &str) -> String {
    if !tag.get(..2).is_some_and(|head| head.eq_ignore_ascii_case("<a")) {
        return tag.to_string();
    }
    // `<abbr>` is not an anchor: the element name has to end here.
    let ends_name = tag
        .get(2..3)
        .is_some_and(|next| next.starts_with(|ch: char| ch.is_ascii_whitespace() || ch == '>'));
    if !ends_name {
        return tag.to_string();
    }
    let attrs = parse_attrs(tag);
    let Some(href) = attrs
        .iter()
        .find(|attr| attr.name.eq_ignore_ascii_case("href"))
        .and_then(|attr| attr.value)
    else {
        return tag.to_string();
    };
    if !is_external(href) {
        return tag.to_string();
    }

    let has_target = attrs
        .iter()
        .any(|attr| attr.name.eq_ignore_ascii_case("target"));
    let rel = attrs
        .iter()
        .find(|attr| attr.name.eq_ignore_ascii_case("rel"));
    let rel_tokens: Vec<&str> = rel
        .and_then(|attr| attr.value)
        .map(|value| value.split_whitespace().collect())
        .unwrap_or_default();
    let add_noopener = !rel_tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("noopener"));
    let add_noreferrer = !rel_tokens
        .iter()
        .any(|token| token.eq_ignore_ascii_case("noreferrer"));
    if has_target && !add_noopener && !add_noreferrer {
        return tag.to_string();
    }

    let mut merged = rel_tokens.clone();
    if add_noopener {
        merged.push("noopener");
    }
    if add_noreferrer {
        merged.push("noreferrer");
    }
    let merged_rel = merged.join(" ");

    let mut out = String::with_capacity(tag.len() + 40);
    out.push_str("<a");
    for attr in &attrs {
        out.push(' ');
        out.push_str(attr.name);
        let value = if attr.name.eq_ignore_ascii_case("rel") {
            Some(merged_rel.clone())
        } else {
            attr.value.map(str::to_string)
        };
        if let Some(value) = value {
            let quote = attr.quote.unwrap_or('"');
            out.push('=');
            out.push(quote);
            out.push_str(&value);
            out.push(quote);
        }
    }
    if !has_target {
        out.push_str(" target=\"_blank\"");
    }
    if rel.is_none() {
        out.push_str(" rel=\"noopener noreferrer\"");
    }
    out.push_str(if tag.ends_with("/>") { "/>" } else { ">" });
    out
}

/// One attribute of a start tag, as it was written.
struct Attr<'a> {
    name: &'a str,
    value: Option<&'a str>,
    /// The quote the value was written with, so a rebuild can keep it.
    quote: Option<char>,
}

/// The attributes of a start tag, in the order they appear.
fn parse_attrs(tag: &str) -> Vec<Attr<'_>> {
    let bytes = tag.as_bytes();
    let mut attrs = Vec::new();
    let mut index = 1;
    // The element name.
    while index < bytes.len()
        && !bytes[index].is_ascii_whitespace()
        && bytes[index] != b'>'
        && bytes[index] != b'/'
    {
        index += 1;
    }
    while index < bytes.len() {
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] == b'>' || bytes[index] == b'/' {
            break;
        }
        let start = index;
        while index < bytes.len()
            && !bytes[index].is_ascii_whitespace()
            && bytes[index] != b'='
            && bytes[index] != b'>'
            && bytes[index] != b'/'
        {
            index += 1;
        }
        if index == start {
            // A stray `=` or quote is not a name: step over it rather than
            // looping on it.
            index += 1;
            continue;
        }
        let name = &tag[start..index];
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            attrs.push(Attr {
                name,
                value: None,
                quote: None,
            });
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index < bytes.len() && (bytes[index] == b'"' || bytes[index] == b'\'') {
            let quote = bytes[index] as char;
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote as u8 {
                index += 1;
            }
            let value = &tag[value_start..index];
            if index < bytes.len() {
                index += 1;
            }
            attrs.push(Attr {
                name,
                value: Some(value),
                quote: Some(quote),
            });
        } else {
            let value_start = index;
            while index < bytes.len()
                && !bytes[index].is_ascii_whitespace()
                && bytes[index] != b'>'
            {
                index += 1;
            }
            attrs.push(Attr {
                name,
                value: Some(&tag[value_start..index]),
                quote: None,
            });
        }
    }
    attrs
}

/// The length of the tag starting at `rest`, up to and including its `>`.
fn tag_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    let mut quote: Option<u8> = None;
    let mut index = 1;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) => {
                if byte == open {
                    quote = None;
                }
            }
            None => match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'>' => return Some(index + 1),
                _ => {}
            },
        }
        index += 1;
    }
    None
}

/// The raw-text element `rest` opens, if it opens one.
fn raw_text_element(rest: &str) -> Option<&'static str> {
    let bytes = rest.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    for name in RAW_TEXT {
        let end = 1 + name.len();
        if bytes.len() < end || !bytes[1..end].eq_ignore_ascii_case(name.as_bytes()) {
            continue;
        }
        match bytes.get(end) {
            Some(byte) if byte.is_ascii_whitespace() || *byte == b'>' || *byte == b'/' => {
                return Some(name);
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_to_another_host_opens_in_a_new_tab() {
        assert_eq!(
            external_links("<a href=\"https://example.com/x\">x</a>"),
            "<a href=\"https://example.com/x\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>"
        );
    }

    #[test]
    fn a_link_within_the_site_stays_in_the_tab() {
        for href in [
            "/blog",
            "/blog/alpha#top",
            "#section",
            "https://ethang.dev/blog/alpha/",
            "https://www.ethang.dev/",
            "//ethang.dev/x",
            "mailto:hello@ethang.email",
            "tel:+15555550100",
        ] {
            let tag = format!("<a href=\"{href}\">x</a>");
            assert_eq!(external_links(&tag), tag, "{href} should not be rewritten");
        }
    }

    #[test]
    fn is_external_reads_the_host_not_the_prefix() {
        assert!(is_external("https://example.com"));
        assert!(is_external("//example.com/path"));
        assert!(is_external("http://example.com:8080/"));
        assert!(is_external("https://user:pw@example.com/"));
        // A host that merely ends in the site's name is another host.
        assert!(is_external("https://notethang.dev/"));
        assert!(is_external("https://ethang.dev.example.com/"));
        assert!(!is_external("https://ethang.dev"));
        assert!(!is_external("https://ETHANG.DEV/x"));
        assert!(!is_external("/relative"));
        assert!(!is_external(""));
    }

    #[test]
    fn an_existing_target_is_not_duplicated() {
        let tag = "<a href=\"https://example.com\" target=\"_self\">x</a>";
        let out = external_links(tag);
        assert_eq!(out.matches("target=").count(), 1, "{out}");
        assert!(out.contains("target=\"_self\""), "{out}");
        assert!(out.contains("rel=\"noopener noreferrer\""), "{out}");
    }

    #[test]
    fn an_existing_rel_keeps_its_tokens() {
        let out = external_links("<a href=\"https://example.com\" rel=\"external\">x</a>");
        assert!(
            out.contains("rel=\"external noopener noreferrer\""),
            "{out}"
        );
    }

    #[test]
    fn a_rel_that_already_has_both_tokens_is_left_alone() {
        let tag = "<a href=\"https://example.com\" target=\"_blank\" rel=\"noopener noreferrer\">x</a>";
        assert_eq!(external_links(tag), tag);
    }

    #[test]
    fn the_rewrite_is_idempotent() {
        let once = external_links("<a href=\"https://example.com\">x</a>");
        assert_eq!(external_links(&once), once);
    }

    #[test]
    fn other_attributes_and_their_order_survive() {
        let out = external_links("<a class=\"c\" href=\"https://example.com\" aria-label=\"A\">x</a>");
        assert!(
            out.starts_with("<a class=\"c\" href=\"https://example.com\" aria-label=\"A\""),
            "{out}"
        );
    }

    #[test]
    fn a_single_quoted_attribute_keeps_its_quote() {
        let out = external_links("<a href='https://example.com'>x</a>");
        assert!(out.contains("href='https://example.com'"), "{out}");
    }

    #[test]
    fn an_anchor_inside_a_code_block_is_not_a_link() {
        let html = "<pre><code>&lt;a href=\"https://example.com\"&gt;</code></pre>";
        assert_eq!(external_links(html), html);
        // Even unescaped, the content of a raw-text element is text.
        let raw = "<code><a href=\"https://example.com\"></code>";
        assert_eq!(external_links(raw), raw);
    }

    #[test]
    fn an_element_whose_name_starts_with_a_is_not_an_anchor() {
        for tag in ["<abbr title=\"x\">y</abbr>", "<aside href=\"https://example.com\">"] {
            assert_eq!(external_links(tag), tag, "{tag}");
        }
    }

    #[test]
    fn an_anchor_without_an_href_is_left_alone() {
        let tag = "<a name=\"top\">x</a>";
        assert_eq!(external_links(tag), tag);
    }

    #[test]
    fn a_link_inside_a_comment_is_left_alone() {
        let html = "<!-- <a href=\"https://example.com\">x</a> -->";
        assert_eq!(external_links(html), html);
    }

    #[test]
    fn a_greater_than_inside_an_attribute_does_not_end_the_tag() {
        let out = external_links("<a title=\"a > b\" href=\"https://example.com\">x</a>");
        assert!(out.contains("target=\"_blank\""), "{out}");
        assert!(out.ends_with(">x</a>"), "{out}");
    }

    #[test]
    fn every_anchor_in_a_document_is_rewritten() {
        let html = "<p><a href=\"https://a.example\">a</a> <a href=\"/b\">b</a> \
                    <a href=\"https://c.example\">c</a></p>";
        let out = external_links(html);
        assert_eq!(out.matches("target=\"_blank\"").count(), 2, "{out}");
    }
}
