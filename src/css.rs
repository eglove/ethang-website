//! Build-time CSS: minification and per-page rule pruning.
//!
//! Every page ships its own copy of the stylesheet, inlined in `<head>`, so the
//! stylesheet is compressed once per page and paid for by every response. Two
//! things make that affordable:
//!
//! - **Minification.** Comments, indentation, and the last semicolon of every
//!   declaration block are gone, and `0.5rem` becomes `.5rem`. The stylesheet
//!   loses about a third of its bytes with no semantic change, which is the
//!   same work a CSS minifier does, so it happens here rather than in a build
//!   step that would need its own toolchain.
//! - **Pruning.** A page that renders no `<table>` has no use for the table
//!   rules. Each rule is kept only when one of its selectors can match the
//!   document it is being inlined into.
//!
//! Pruning is deliberately conservative, because the failure mode is invisible:
//! a dropped rule does not error, it just renders wrong. A rule is dropped only
//! when *every* selector in its prelude names at least one class, id, or
//! attribute that appears nowhere in the page - that is, when the selector is
//! provably unmatchable rather than merely unmatched. Tag-only selectors, `:root`,
//! `*`, pseudo-classes, and anything this module does not fully understand are
//! always kept. `tests/css_pruning.rs` checks the consequence: the pruned
//! stylesheet and the full stylesheet must resolve the same computed style for
//! every element on the page.

/// One CSS block: a rule, or an at-rule with its body still attached.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    /// A plain rule. `selectors` is the prelude split on commas, `body` the
    /// declarations between the braces.
    Rule { selectors: Vec<String>, body: String },
    /// An at-rule kept whole: `@font-face`, `@media`, and anything else whose
    /// inner blocks are not selectors in the same sense.
    AtRule { prelude: String, body: String },
    /// A top-level comment, which minification discards.
    Comment,
}

/// Strip comments, collapse whitespace, and drop the redundant parts of a
/// stylesheet. The result parses to the same rules as the input.
pub fn minify(css: &str) -> String {
    let nodes = parse(css);
    let mut out = String::with_capacity(css.len());
    for node in &nodes {
        match node {
            Node::Comment => {}
            Node::Rule { selectors, body } => {
                let selectors: Vec<String> = selectors.iter().map(|s| minify_selector(s)).collect();
                out.push_str(&selectors.join(","));
                out.push('{');
                out.push_str(&minify_declarations(body));
                out.push('}');
            }
            Node::AtRule { prelude, body } => {
                out.push_str(&minify_at_prelude(prelude));
                out.push('{');
                // `@media` and `@supports` wrap rules, which minify recursively;
                // `@font-face` and the rest wrap declarations.
                if wraps_rules(prelude) {
                    out.push_str(&minify(body));
                } else {
                    out.push_str(&minify_declarations(body));
                }
                out.push('}');
            }
        }
    }
    out
}

/// Minify `css`, then drop every rule that cannot match `document`.
///
/// `document` is the page's rendered HTML. A rule is dropped only when none of
/// its selectors can match: see the module docs for why that is the safe test.
pub fn minify_for(css: &str, document: &str) -> String {
    let nodes = parse(css);
    let mut out = String::with_capacity(css.len() / 2);
    for node in &nodes {
        match node {
            Node::Comment => {}
            Node::Rule { selectors, body } => {
                let kept: Vec<&String> = selectors
                    .iter()
                    .filter(|selector| could_match(selector, document))
                    .collect();
                if kept.is_empty() {
                    continue;
                }
                let kept: Vec<String> = kept.into_iter().map(|s| minify_selector(s)).collect();
                out.push_str(&kept.join(","));
                out.push('{');
                out.push_str(&minify_declarations(body));
                out.push('}');
            }
            Node::AtRule { prelude, body } => {
                if prelude.starts_with("@font-face") {
                    // The faces are named by family, not by selector: whether a
                    // page uses one is decided by the `font-family` declarations
                    // the other rules carry, so they are all kept.
                    out.push_str(&minify_at_prelude(prelude));
                    out.push('{');
                    out.push_str(&minify_declarations(body));
                    out.push('}');
                    continue;
                }
                if !wraps_rules(prelude) {
                    out.push_str(&minify_at_prelude(prelude));
                    out.push('{');
                    out.push_str(&minify_declarations(body));
                    out.push('}');
                    continue;
                }
                let inner = minify_for(body, document);
                if inner.is_empty() {
                    continue;
                }
                out.push_str(&minify_at_prelude(prelude));
                out.push('{');
                out.push_str(&inner);
                out.push('}');
            }
        }
    }
    out
}

/// Whether a single selector could match anything in `document`.
///
/// The test is intentionally one-sided. A selector made only of tag names,
/// pseudo-classes, and combinators is kept, because a tag can be added by a
/// post's markdown and the pruner cannot see that. A selector that names a
/// class, id, or attribute is kept when any of those tokens occurs anywhere in
/// the document - occurrence in the text is a superset of occurrence as a
/// class, id, or attribute value, so the test never drops a rule that matches.
fn could_match(selector: &str, document: &str) -> bool {
    let mut tokens = 0usize;
    let mut found = 0usize;
    let bytes = selector.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let marker = bytes[index];
        // `::selection` and `:focus-visible` are pseudo-elements and
        // pseudo-classes, not ids: the second colon of a pseudo-element would
        // otherwise read as one, and `::selection` would be dropped from every
        // page for naming an id that does not exist.
        if marker == b':' {
            index += 1;
            while index < bytes.len() && bytes[index] == b':' {
                index += 1;
            }
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'-')
            {
                index += 1;
            }
            continue;
        }
        if marker != b'.' && marker != b'#' && marker != b'[' {
            index += 1;
            continue;
        }
        // A `.` or `#` inside a declaration or a string is not a selector, but
        // the prelude has already been stripped of those by the parser.
        let mut end = index + 1;
        while end < bytes.len() {
            let byte = bytes[end];
            let is_name = byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_';
            if marker == b'[' {
                // `[name]`, `[name=value]`, `[name~=value i]` - the name is
                // what identifies the attribute, so it is what gets looked for.
                if !is_name {
                    break;
                }
            } else if !is_name {
                break;
            }
            end += 1;
        }
        if end == index + 1 {
            // A lone `.` or `#` is not a name: `.5rem` reaches here only if the
            // parser mis-split a declaration, and guessing would be worse than
            // keeping the rule.
            return true;
        }
        let name = &selector[index + 1..end];
        tokens += 1;
        if document.contains(name) {
            found += 1;
        }
        index = end;
    }
    if tokens == 0 {
        // No class, id, or attribute: a tag or a pseudo-class. Kept.
        return true;
    }
    // Every selector in a comma list must be independently viable, and a
    // selector that names several tokens matches only if all of them occur.
    found == tokens
}

/// Split a stylesheet into its top-level nodes.
fn parse(css: &str) -> Vec<Node> {
    let mut nodes = Vec::new();
    let bytes = css.as_bytes();
    let mut index = 0usize;
    let mut prelude_start = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                // A comment outside a block is its own node; one inside a
                // prelude is dropped by the prelude trimmer.
                let end = find(bytes, index + 2, b"*/").map_or(bytes.len(), |at| at + 2);
                let prelude = css[prelude_start..index].trim();
                if !prelude.is_empty() {
                    // A comment between rules ends the preceding prelude run.
                    push_prelude(&mut nodes, prelude);
                }
                nodes.push(Node::Comment);
                index = end;
                prelude_start = index;
            }
            b'{' => {
                let prelude = strip_comments(css[prelude_start..index].trim());
                let (body, next) = block_body(css, index + 1);
                if prelude.starts_with('@') {
                    nodes.push(Node::AtRule { prelude, body: body.to_string() });
                } else {
                    nodes.push(Node::Rule {
                        selectors: split_selectors(&prelude),
                        body: body.to_string(),
                    });
                }
                index = next;
                prelude_start = index;
            }
            _ => index += 1,
        }
    }
    nodes
}

/// Text between two blocks is not valid CSS; keep it rather than lose it.
fn push_prelude(nodes: &mut Vec<Node>, prelude: &str) {
    let prelude = strip_comments(prelude);
    if prelude.is_empty() {
        return;
    }
    if prelude.starts_with('@') {
        nodes.push(Node::AtRule { prelude, body: String::new() });
    } else {
        nodes.push(Node::Rule { selectors: split_selectors(&prelude), body: String::new() });
    }
}

/// The body of a block whose opening brace is at `start - 1`, and the offset
/// just past its closing brace.
fn block_body(css: &str, start: usize) -> (&str, usize) {
    let bytes = css.as_bytes();
    let mut depth = 1usize;
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = find(bytes, index + 2, b"*/").map_or(bytes.len(), |at| at + 2);
                continue;
            }
            b'"' | b'\'' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    if bytes[index] == b'\\' {
                        index += 1;
                    }
                    index += 1;
                }
                index += 1;
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return (&css[start..index], index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    (&css[start..], bytes.len())
}

fn find(bytes: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|at| at + from)
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("/*") {
        out.push_str(&rest[..at]);
        match rest[at + 2..].find("*/") {
            Some(end) => rest = &rest[at + 2 + end + 2..],
            // An unterminated comment swallows the rest, as a browser does.
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Split a prelude on the commas that separate selectors, leaving the ones
/// inside `:is(...)`, `:not(...)`, and `[attr="a,b"]` alone.
fn split_selectors(prelude: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    for (index, ch) in prelude.char_indices() {
        match ch {
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
            }
            '(' | '[' if quote.is_none() => depth += 1,
            ')' | ']' if quote.is_none() => depth -= 1,
            ',' if depth == 0 && quote.is_none() => {
                out.push(prelude[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(prelude[start..].trim().to_string());
    out.retain(|selector| !selector.is_empty());
    out
}

/// Collapse the whitespace inside one selector.
///
/// The descendant combinator is a single space; the spaces around `>`, `+`, and
/// `~` are not part of the selector, and neither are the ones inside brackets or
/// parentheses.
fn minify_selector(selector: &str) -> String {
    let mut out = String::with_capacity(selector.len());
    let mut pending_space = false;
    let mut combinator = false;
    let mut quote: Option<char> = None;
    let mut depth = 0i32;
    for ch in selector.trim().chars() {
        if let Some(open) = quote {
            out.push(ch);
            if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                quote = Some(ch);
                out.push(ch);
            }
            '(' | '[' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                depth += 1;
                out.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                pending_space = false;
                out.push(ch);
            }
            '>' | '+' | '~' if depth == 0 => {
                while out.ends_with(' ') {
                    out.pop();
                }
                pending_space = false;
                out.push(ch);
                // The space after a combinator is not part of the selector.
                combinator = true;
            }
            ch if ch.is_whitespace() => pending_space = true,
            _ => {
                if pending_space && !out.is_empty() && !combinator {
                    out.push(' ');
                }
                pending_space = false;
                combinator = false;
                out.push(ch);
            }
        }
    }
    out
}

/// Minify a declaration block: one declaration per `;`, no trailing `;`,
/// no space around `:`, and a leading zero dropped from a length.
fn minify_declarations(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0usize;
    let push = |out: &mut String, declaration: &str, first: bool| {
        let declaration = minify_declaration(declaration);
        if declaration.is_empty() {
            return;
        }
        if !first {
            out.push(';');
        }
        out.push_str(&declaration);
    };
    let mut first = true;
    for (index, ch) in body.char_indices() {
        match ch {
            '"' | '\'' => {
                if quote == Some(ch) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(ch);
                }
            }
            '(' if quote.is_none() => depth += 1,
            ')' if quote.is_none() => depth -= 1,
            ';' if depth == 0 && quote.is_none() => {
                push(&mut out, &body[start..index], first);
                if !out.is_empty() {
                    first = false;
                }
                start = index + 1;
            }
            _ => {}
        }
    }
    push(&mut out, &body[start..], first);
    out
}

fn minify_declaration(declaration: &str) -> String {
    let declaration = declaration.trim();
    if declaration.is_empty() {
        return String::new();
    }
    let Some((name, value)) = declaration.split_once(':') else {
        return declaration.to_string();
    };
    let name = name.trim();
    let value = minify_value(value.trim());
    if value.is_empty() {
        return format!("{name}:");
    }
    format!("{name}:{value}")
}

/// Collapse a declaration value: one space between words, none inside
/// `rgba(...)`, and no leading zero on a fractional length.
fn minify_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut quote: Option<char> = None;
    let mut pending_space = false;
    for ch in value.chars() {
        if let Some(open) = quote {
            out.push(ch);
            if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                quote = Some(ch);
                out.push(ch);
            }
            ch if ch.is_whitespace() => pending_space = true,
            ',' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                pending_space = false;
                out.push(',');
            }
            '(' => {
                pending_space = false;
                out.push('(');
            }
            ')' => {
                while out.ends_with(' ') {
                    out.pop();
                }
                pending_space = false;
                out.push(')');
            }
            _ => {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(ch);
            }
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    strip_leading_zeros(&out)
}

/// `0.5rem` becomes `.5rem`; `10.5rem` and `#00ff00` are left alone.
///
/// A leading zero only goes when it starts a number, which is when the
/// character before it is not part of one.
fn strip_leading_zeros(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '0'
            && chars.peek() == Some(&'.')
            && out
                .chars()
                .last()
                .map(|previous| !previous.is_ascii_alphanumeric() && previous != '.')
                .unwrap_or(true)
        {
            continue;
        }
        out.push(ch);
    }
    out
}

fn minify_at_prelude(prelude: &str) -> String {
    let prelude = strip_comments(prelude.trim());
    let mut out = String::with_capacity(prelude.len());
    let mut pending_space = false;
    // No space is meaningful next to a parenthesis or a colon in an at-rule
    // prelude: `@media (min-width: 40rem)` and `@media(min-width:40rem)` are
    // the same query.
    let mut tight = false;
    for ch in prelude.chars() {
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        let skip_space = tight
            || out.is_empty()
            || out.ends_with('(')
            || ch == ')'
            || ch == ':'
            || ch == ',';
        if pending_space && !skip_space {
            out.push(' ');
        }
        pending_space = false;
        tight = ch == ':' || ch == ',';
        out.push(ch);
    }
    out
}

fn wraps_rules(prelude: &str) -> bool {
    let prelude = prelude.trim_start();
    prelude.starts_with("@media") || prelude.starts_with("@supports") || prelude.starts_with("@layer")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minify_drops_comments_and_collapses_whitespace() {
        let css = "/* a */\n.a {\n  color: red;\n  padding: 0 1rem;\n}\n";
        assert_eq!(minify(css), ".a{color:red;padding:0 1rem}");
    }

    #[test]
    fn minify_keeps_the_last_declaration_without_a_semicolon() {
        assert_eq!(minify(".a{color:red;}"), ".a{color:red}");
    }

    #[test]
    fn minify_collapses_selector_whitespace() {
        assert_eq!(minify(".a > .b , .c  .d{color:red}"), ".a>.b,.c .d{color:red}");
    }

    #[test]
    fn minify_drops_a_leading_zero_on_a_fraction() {
        assert_eq!(minify(".a{margin:0.5rem}"), ".a{margin:.5rem}");
        assert_eq!(minify(".a{margin:0}"), ".a{margin:0}");
    }

    #[test]
    fn minify_keeps_at_rules_and_their_bodies() {
        let css = "@media (min-width: 40rem) { .a { color: red; } }";
        assert_eq!(minify(css), "@media (min-width:40rem){.a{color:red}}");
    }

    #[test]
    fn minify_keeps_a_string_containing_braces() {
        let css = ".a{content:\"{}\"}";
        assert_eq!(minify(css), css);
    }

    #[test]
    fn minify_keeps_font_face_declarations() {
        let css = "@font-face{font-family:\"Inter\";src:url(\"/fonts/inter-400.woff2\") format(\"woff2\")}";
        assert_eq!(minify(css), css);
    }

    #[test]
    fn pruning_drops_a_rule_whose_class_is_absent() {
        let css = ".present{color:red}.absent{color:blue}";
        let pruned = minify_for(css, "<p class=\"present\">x</p>");
        assert_eq!(pruned, ".present{color:red}");
    }

    #[test]
    fn pruning_keeps_a_tag_only_rule() {
        let css = "table{border-collapse:collapse}";
        assert_eq!(minify_for(css, "<p>no table here</p>"), css);
    }

    #[test]
    fn pruning_keeps_a_selector_when_only_some_tokens_appear() {
        // A selector that names two classes needs both; naming one is not
        // enough, and the rule is kept only when the whole selector is viable.
        let css = ".a.b{color:red}";
        assert_eq!(minify_for(css, "class=\"a\""), "");
        assert_eq!(minify_for(css, "class=\"a b\""), css);
    }

    #[test]
    fn pruning_keeps_a_comma_list_that_partly_matches() {
        let css = ".gone,.kept{color:red}";
        assert_eq!(minify_for(css, "class=\"kept\""), ".kept{color:red}");
    }

    #[test]
    fn pruning_keeps_the_font_faces() {
        let css = "@font-face{font-family:\"Inter\";src:url(\"/fonts/inter-400.woff2\") format(\"woff2\")}";
        assert_eq!(minify_for(css, "<p>x</p>"), css);
    }

    #[test]
    fn pruning_drops_a_media_block_whose_rules_all_go() {
        let css = "@media (min-width: 40rem){.gone{color:red}}";
        assert_eq!(minify_for(css, "<p>x</p>"), "");
    }

    #[test]
    fn pruning_keeps_a_media_block_that_still_has_a_rule() {
        let css = "@media (min-width: 40rem){.gone{color:red}.kept{color:blue}}";
        assert_eq!(minify_for(css, "class=\"kept\""), "@media (min-width:40rem){.kept{color:blue}}");
    }

    #[test]
    fn pruning_keeps_an_attribute_selector_whose_name_appears() {
        let css = "[data-blog-post]{color:red}";
        assert_eq!(minify_for(css, "<article data-blog-post></article>"), css);
        assert_eq!(minify_for(css, "<article></article>"), "");
    }
}
