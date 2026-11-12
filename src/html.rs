//! HTML construction. Every text node and attribute value passes through
//! `escape`; `raw` is the only unescaped path and takes trusted markup only.

/// Escape the five characters that can change how markup is parsed.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Trusted markup passthrough. Callers are responsible for what goes in.
pub fn raw(markup: &str) -> String {
    markup.to_string()
}

fn attrs_to_string(attrs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for (name, value) in attrs {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        out.push_str(&escape(value));
        out.push('"');
    }
    out
}

/// An element with an open tag, its children, and a closing tag.
///
/// Children are already-rendered HTML, so text children must be escaped by the
/// caller (`escape` or another builder call); attribute values are escaped here.
pub fn el(tag: &str, attrs: &[(&str, &str)], children: &[String]) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(tag);
    out.push_str(&attrs_to_string(attrs));
    out.push('>');
    for child in children {
        out.push_str(child);
    }
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
    out
}

/// A void element: open tag only, no children, no closing tag.
pub fn void_el(tag: &str, attrs: &[(&str, &str)]) -> String {
    let mut out = String::new();
    out.push('<');
    out.push_str(tag);
    out.push_str(&attrs_to_string(attrs));
    out.push('>');
    out
}

/// Tags whose content is text, not markup, and whose whitespace matters.
const RAW_TEXT: [&str; 3] = ["pre", "textarea", "style"];

/// Elements that open a new block, so whitespace around them is layout, not
/// content. A newline between two of these renders as nothing; a newline
/// between two inline elements renders as a space.
const BLOCK: [&str; 32] = [
    "address", "article", "aside", "blockquote", "dd", "details", "div", "dl", "dt", "fieldset",
    "figcaption", "figure", "footer", "form", "h1", "h2", "h3", "h4", "h5", "h6", "header",
    "hr", "li", "main", "nav", "ol", "p", "section", "table", "tbody", "td", "ul",
];

/// Minify a rendered document without changing what it renders.
///
/// The rules are narrow on purpose, because HTML has no single answer to
/// "does this whitespace matter":
///
/// - Text inside `pre`, `textarea`, and `style` is copied byte for byte.
/// - A whitespace-only run between two tags is dropped when either neighbour is
///   a block element, and collapsed to one space otherwise. That is exactly the
///   distinction the parser makes, so a run of inline elements keeps the space
///   that separates them and a run of blocks loses the newline that separated
///   them.
/// - Every other text run is copied as written, so a paragraph's own spacing
///   survives.
/// - Attribute values are never touched: they sit inside a tag, and tags are
///   copied verbatim.
pub fn minify(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut rest = markup;
    // The name of the element that most recently opened, for the block test.
    let mut previous: Option<String> = None;

    while let Some(at) = rest.find('<') {
        let text = &rest[..at];
        if !text.is_empty() {
            out.push_str(&collapse(text, previous.as_deref(), next_tag_name(&rest[at..]).as_deref()));
        }
        let Some(close) = rest[at..].find('>') else {
            out.push_str(&rest[at..]);
            return out;
        };
        let tag = &rest[at..at + close + 1];
        out.push_str(tag);
        rest = &rest[at + close + 1..];

        let name = tag_name(tag);
        if let Some(name) = &name {
            if is_raw_text(name) {
                // Copy the element's content verbatim up to its closing tag.
                let close_tag = format!("</{name}");
                match find_ascii_case_insensitive(rest, &close_tag) {
                    Some(end) => {
                        out.push_str(&rest[..end]);
                        rest = &rest[end..];
                        // The closing tag itself is handled by the next pass.
                        if let Some(close) = rest.find('>') {
                            out.push_str(&rest[..close + 1]);
                            rest = &rest[close + 1..];
                        }
                    }
                    None => {
                        out.push_str(rest);
                        return out;
                    }
                }
                previous = Some(name.clone());
                continue;
            }
            previous = Some(name.clone());
        }
    }
    out.push_str(rest);
    // The document ends with a newline the shell wrote; it is not content.
    while out.ends_with('\n') || out.ends_with('\r') {
        out.pop();
    }
    out
}

/// The element name a tag opens or closes, if it names one.
fn tag_name(tag: &str) -> Option<String> {
    let inner = tag.strip_prefix('<')?;
    if let Some(closing) = inner.strip_prefix('/') {
        // A closing tag names the element that just ended, which is the one the
        // next block test should see.
        let name: String = closing
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric())
            .collect();
        return (!name.is_empty()).then(|| name.to_ascii_lowercase());
    }
    let name: String = inner
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    (!name.is_empty()).then(|| name.to_ascii_lowercase())
}

/// The name of the first tag in `source`, which starts at `<`.
fn next_tag_name(source: &str) -> Option<String> {
    let close = source.find('>')?;
    tag_name(&source[..close + 1])
}

fn is_raw_text(name: &str) -> bool {
    RAW_TEXT.contains(&name)
}

fn is_block(name: Option<&str>) -> bool {
    name.map(|name| BLOCK.contains(&name)).unwrap_or(false)
}

/// What to emit for a text run, given the tags on either side of it.
fn collapse(text: &str, before: Option<&str>, after: Option<&str>) -> String {
    if text.trim().is_empty() {
        if is_block(before) || is_block(after) {
            return String::new();
        }
        return " ".to_string();
    }
    text.to_string()
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&index| {
        haystack[index..index + needle.len()].eq_ignore_ascii_case(needle)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_all_five_dangerous_characters() {
        assert_eq!(
            escape(r#"<a href="x">&'"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn el_escapes_text_children() {
        let out = el("p", &[], &[escape("a & b")]);
        assert_eq!(out, "<p>a &amp; b</p>");
    }

    #[test]
    fn el_escapes_attribute_values() {
        let out = el("a", &[("href", "/x?a=1&b=2")], &[]);
        assert_eq!(out, "<a href=\"/x?a=1&amp;b=2\"></a>");
    }

    #[test]
    fn void_el_has_no_closing_tag() {
        assert_eq!(void_el("meta", &[("name", "a")]), "<meta name=\"a\">");
    }

    #[test]
    fn raw_passes_markup_through_unescaped() {
        assert_eq!(raw("<b>bold</b>"), "<b>bold</b>");
    }

    #[test]
    fn el_renders_children_in_order() {
        let out = el(
            "div",
            &[("class", "x")],
            &[el("p", &[], &[]), el("p", &[], &[])],
        );
        assert_eq!(out, "<div class=\"x\"><p></p><p></p></div>");
    }

    #[test]
    fn minify_drops_whitespace_between_blocks() {
        assert_eq!(
            minify("<body>\n  <p>x</p>\n  <p>y</p>\n</body>\n"),
            "<body><p>x</p><p>y</p></body>"
        );
    }

    #[test]
    fn minify_keeps_one_space_between_inline_elements() {
        assert_eq!(
            minify("<p><a href=\"/x\">x</a>\n<a href=\"/y\">y</a></p>"),
            "<p><a href=\"/x\">x</a> <a href=\"/y\">y</a></p>"
        );
    }

    #[test]
    fn minify_leaves_text_runs_alone() {
        assert_eq!(minify("<p>a  b</p>"), "<p>a  b</p>");
    }

    #[test]
    fn minify_preserves_preformatted_content() {
        let html = "<pre class=\"code\"><code>let a = 1;\n\n    let b = 2;</code></pre>";
        assert_eq!(minify(html), html);
    }

    #[test]
    fn minify_preserves_a_style_block() {
        let html = "<style>.a{color:red} .b{color:blue}</style>";
        assert_eq!(minify(html), html);
    }

    #[test]
    fn minify_preserves_attribute_values() {
        let html = "<a href=\"/x?a=1&amp;b=2\" title=\"a  b\">x</a>";
        assert_eq!(minify(html), html);
    }

    #[test]
    fn minify_keeps_the_doctype() {
        assert!(minify("<!doctype html>\n<html lang=\"en\">\n</html>\n").starts_with("<!doctype html>"));
    }

    #[test]
    fn minify_does_not_eat_a_comment() {
        assert_eq!(minify("<div><!-- note --></div>"), "<div><!-- note --></div>");
    }
}
