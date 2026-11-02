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
}
