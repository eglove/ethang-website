//! The pruned stylesheet must not change what a page renders.
//!
//! `src/css.rs` drops a rule when none of its selectors can match the page it is
//! being inlined into. That is a claim about the browser, and a wrong claim is
//! invisible: the page still loads, it just looks wrong, and no other test in
//! this suite would notice.
//!
//! So this test re-derives the answer independently of the pruner. It parses
//! each built page into its elements, matches every rule in the full stylesheet
//! against every element, and asserts that the pruned stylesheet matches
//! exactly the same set. Two implementations of the same question, agreeing.
//!
//! The matcher here is deliberately more permissive than a browser: `+` and `~`
//! are treated as descendant combinators, `:not()` and `:hover` are ignored, and
//! every `@media` rule is treated as unconditional. Being permissive can only
//! make this test stricter - it can report a rule as matching when a browser
//! would not, never the reverse.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn dist() -> PathBuf {
    root().join("dist")
}

/// One element's identity, as a selector can name it.
#[derive(Debug, Default, Clone)]
struct Simple {
    tag: String,
    classes: Vec<String>,
    id: Option<String>,
    attrs: Vec<String>,
}

/// The elements of a document, each with its ancestor chain (root first).
fn elements(html: &str) -> Vec<Vec<Simple>> {
    const VOID: [&str; 14] = [
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    let mut out = Vec::new();
    let mut stack: Vec<Simple> = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => {
                    rest = &rest[end + 3..];
                    continue;
                }
                None => break,
            }
        }
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close + 1];
        rest = &rest[close + 1..];
        if tag.starts_with("<!") || tag.starts_with("<?") {
            continue;
        }
        if let Some(name) = tag.strip_prefix("</") {
            let name = name.trim_end_matches('>').trim().to_ascii_lowercase();
            if let Some(position) = stack.iter().rposition(|element| element.tag == name) {
                stack.truncate(position);
            }
            continue;
        }
        let element = parse_element(tag);
        out.push(stack.clone());
        // `<html>` and `<body>` are written without their end tags, so the
        // parser has to close them implicitly: an element that cannot be a
        // child of the open one ends it. Without this the whole page nests
        // inside `<html>` and every descendant selector matches.
        let self_closing = tag.ends_with("/>") || VOID.contains(&element.tag.as_str());
        if self_closing {
            continue;
        }
        if element.tag == "body" || element.tag == "head" {
            stack.clear();
        }
        stack.push(element);
    }
    out
}

fn parse_element(tag: &str) -> Simple {
    let inner = tag.trim_start_matches('<').trim_end_matches('>');
    let mut element = Simple::default();
    let name_end = inner
        .find(|ch: char| ch.is_whitespace() || ch == '/')
        .unwrap_or(inner.len());
    element.tag = inner[..name_end].to_ascii_lowercase();
    let mut rest = &inner[name_end..];
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim();
        let name = name.rsplit(|ch: char| ch.is_whitespace()).next().unwrap_or(name);
        let after = &rest[eq + 1..];
        let Some(quote) = after.chars().next() else { break };
        if quote != '"' && quote != '\'' {
            rest = after;
            continue;
        }
        let Some(end) = after[1..].find(quote) else { break };
        let value = &after[1..1 + end];
        match name {
            "class" => {
                for class in value.split_whitespace() {
                    element.classes.push(class.to_string());
                }
            }
            "id" => element.id = Some(value.to_string()),
            other => element.attrs.push(other.to_ascii_lowercase()),
        }
        rest = &after[2 + end..];
    }
    element
}

/// A selector split into its compounds, rightmost last.
fn compounds(selector: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for ch in selector.chars() {
        if let Some(open) = quote {
            current.push(ch);
            if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            '>' | '+' | '~' if depth == 0 => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                }
                current.clear();
            }
            ch if ch.is_whitespace() && depth == 0 => {
                if !current.trim().is_empty() {
                    out.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// Whether one compound selector names this element.
fn compound_matches(compound: &str, element: &Simple) -> bool {
    let mut rest = compound;
    // The tag, if the compound starts with one.
    let tag_end = rest
        .find(['.', '#', '[', ':'])
        .unwrap_or(rest.len());
    let tag = &rest[..tag_end];
    if !tag.is_empty() && tag != "*" && !tag.eq_ignore_ascii_case(&element.tag) {
        return false;
    }
    rest = &rest[tag_end..];
    while !rest.is_empty() {
        let mut chars = rest.chars();
        match chars.next() {
            Some('.') => {
                let end = rest[1..]
                    .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
                    .map_or(rest.len(), |at| at + 1);
                let class = &rest[1..end];
                if !element.classes.iter().any(|name| name == class) {
                    return false;
                }
                rest = &rest[end..];
            }
            Some('#') => {
                let end = rest[1..]
                    .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
                    .map_or(rest.len(), |at| at + 1);
                let id = &rest[1..end];
                if element.id.as_deref() != Some(id) {
                    return false;
                }
                rest = &rest[end..];
            }
            Some('[') => {
                let Some(end) = rest.find(']') else { return true };
                let name: String = rest[1..end]
                    .chars()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
                    .collect::<String>()
                    .to_ascii_lowercase();
                if !element.attrs.contains(&name) {
                    return false;
                }
                rest = &rest[end + 1..];
            }
            // A pseudo-class or pseudo-element: ignored, which is the
            // permissive direction.
            Some(':') => {
                let end = rest[1..]
                    .find(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '(')
                    .map_or(rest.len(), |at| at + 1);
                let mut rest_after = &rest[end..];
                if rest_after.starts_with('(') {
                    let mut depth = 0i32;
                    let mut consumed = 0;
                    for (index, ch) in rest_after.char_indices() {
                        if ch == '(' {
                            depth += 1;
                        } else if ch == ')' {
                            depth -= 1;
                            if depth == 0 {
                                consumed = index + 1;
                                break;
                            }
                        }
                    }
                    rest_after = &rest_after[consumed..];
                }
                rest = rest_after;
            }
            _ => return true,
        }
    }
    true
}

/// Whether a selector matches the element at `chain[index]`, given its ancestors.
fn selector_matches(compounds: &[String], chain: &[Simple], index: isize) -> bool {
    let Some(compound) = compounds.last() else {
        return true;
    };
    if index < 0 {
        return false;
    }
    if !compound_matches(compound, &chain[index as usize]) {
        return false;
    }
    if compounds.len() == 1 {
        return true;
    }
    // Any ancestor may satisfy the rest; the combinators are treated as
    // descendant, which is the permissive reading.
    (0..index).rev().any(|up| selector_matches(&compounds[..compounds.len() - 1], chain, up))
}

/// One rule: its selectors and its declaration block.
///
/// The declaration text is kept for the failure message only. Minification
/// rewrites it - whitespace, the trailing semicolon - so comparing bodies would
/// report every rule as different; what has to be identical is the set of
/// selectors that match.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Rule {
    selectors: Vec<String>,
    body: String,
}

/// Every style rule in a stylesheet, with `@media` unwrapped.
fn rules(css: &str) -> Vec<Rule> {
    let mut out = Vec::new();
    collect_rules(css, &mut out);
    out
}

fn collect_rules(css: &str, out: &mut Vec<Rule>) {
    let bytes = css.as_bytes();
    let mut index = 0usize;
    let mut prelude_start = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = css[index + 2..]
                    .find("*/")
                    .map_or(bytes.len(), |at| index + 2 + at + 2);
                prelude_start = index;
            }
            b'{' => {
                let prelude = css[prelude_start..index].trim().to_string();
                let mut depth = 1usize;
                let mut end = index + 1;
                while end < bytes.len() && depth > 0 {
                    match bytes[end] {
                        b'/' if bytes.get(end + 1) == Some(&b'*') => {
                            end = css[end + 2..]
                                .find("*/")
                                .map_or(bytes.len(), |at| end + 2 + at + 2);
                            continue;
                        }
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    end += 1;
                }
                let body = css[index + 1..end.saturating_sub(1)].to_string();
                if prelude.starts_with("@media") || prelude.starts_with("@supports") {
                    collect_rules(&body, out);
                } else if !prelude.starts_with('@') && !prelude.is_empty() {
                    out.push(Rule {
                        selectors: prelude.split(',').map(|s| s.trim().to_string()).collect(),
                        body: body.trim().to_string(),
                    });
                }
                index = end;
                prelude_start = index;
            }
            _ => index += 1,
        }
    }
}

/// The stylesheet a page inlined into its own `<style>` element.
fn inlined(html: &str) -> String {
    let start = html
        .find("<style>")
        .unwrap_or_else(|| panic!("no inline stylesheet in the page"));
    let end = html[start..]
        .find("</style>")
        .map(|at| start + at)
        .expect("unterminated style element");
    html[start + "<style>".len()..end].to_string()
}

fn pages() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dist()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The selectors of `sheet` that match at least one element of `document`.
fn matching(sheet: &str, document: &[Vec<Simple>]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for rule in rules(sheet) {
        for selector in &rule.selectors {
            let parts = compounds(selector);
            if parts.is_empty() {
                continue;
            }
            if document
                .iter()
                .any(|chain| selector_matches(&parts, chain, chain.len() as isize - 1))
            {
                out.insert(selector.clone());
            }
        }
    }
    out
}

#[test]
fn the_pruned_stylesheet_matches_exactly_what_the_full_one_matches() {
    let full = std::fs::read_to_string(root().join("static/styles/site.css"))
        .expect("reading static/styles/site.css");
    let pages = pages();
    assert!(
        !pages.is_empty(),
        "no built pages: run `cargo run --release -- build` first"
    );
    for page in &pages {
        let html = std::fs::read_to_string(page).unwrap();
        let document = elements(&html);
        let pruned = inlined(&html);
        let expected = matching(&full, &document);
        let actual = matching(&pruned, &document);
        let dropped: Vec<&String> = expected.difference(&actual).collect();
        assert!(
            dropped.is_empty(),
            "{} lost selectors that match it: {:#?}",
            page.display(),
            dropped
        );
        // The pruner must not invent a rule either: the inlined sheet is a
        // subset of the full one, selector for selector.
        let invented: Vec<&String> = actual.difference(&expected).collect();
        assert!(
            invented.is_empty(),
            "{} carries selectors the full sheet does not: {:#?}",
            page.display(),
            invented
        );
    }
}

#[test]
fn every_page_inlines_a_stylesheet_and_the_site_ships_no_css_file() {
    for page in pages() {
        let html = std::fs::read_to_string(&page).unwrap();
        assert!(
            html.contains("<style>"),
            "{} has no inline stylesheet",
            page.display()
        );
        assert!(
            !html.contains("rel=\"stylesheet\""),
            "{} still links an external stylesheet",
            page.display()
        );
    }
    assert!(
        !dist().join("styles/site.css").exists(),
        "the authored stylesheet is a build input and must not be published"
    );
}

#[test]
fn a_post_page_ships_less_css_than_the_whole_sheet() {
    let full = std::fs::read_to_string(root().join("static/styles/site.css")).unwrap();
    let html = std::fs::read_to_string(dist().join("blog/index.html")).unwrap();
    let pruned = inlined(&html);
    assert!(
        pruned.len() < full.len() / 2,
        "the inlined stylesheet is {} bytes against {} authored: pruning did nothing",
        pruned.len(),
        full.len()
    );
}
