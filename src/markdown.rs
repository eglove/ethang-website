//! Markdown to HTML via pulldown-cmark, including the MDX component expander
//! that rewrites PostImage, PostTable, Blockquote, and VideoEmbed before parsing.
//!
//! The posts were authored as MDX, but little of that syntax has to survive:
//! each component invocation is rewritten into the HTML it used to render,
//! and pulldown-cmark parses the rest as ordinary markdown. That
//! keeps the post bodies readable as prose in `content/` while the output is
//! plain HTML with no JavaScript anywhere.
//!
//! Three rewrites happen before the parser runs: the four components plus
//! `Heading` become their elements, the `import` lines that bind post assets
//! are collected into a lookup table, and JSX-only attribute syntax
//! (`style={{ ... }}`, `className`) becomes the HTML equivalent.

use crate::html::{el, escape, void_el};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};
use std::collections::BTreeMap;

/// Everything a single post body needs from the rest of the build.
///
/// `highlight` takes `(language, code)`; `render_math` takes `(latex, display)`.
/// Both are injected so markdown rendering stays testable without syntect or
/// latex2mathml, and so Tasks 5 and 6 can fill them in without touching this file.
pub struct RenderCtx<'a> {
    pub post_slug: &'a str,
    pub highlight: &'a dyn Fn(&str, &str) -> String,
    pub render_math: &'a dyn Fn(&str, bool) -> String,
}

/// Placeholder openers that cannot occur in prose. The private-use area is
/// safe because pulldown-cmark passes these code points through untouched.
const INLINE_MATH: char = '\u{e000}';
const DISPLAY_MATH: char = '\u{e001}';

/// Render a post body to HTML.
pub fn render_markdown(body: &str, ctx: &RenderCtx<'_>) -> String {
    let expanded =
        expand_components(body).replace(ASSET_ROOT, &format!("/images/blog/{}/", ctx.post_slug));
    let (source, mut math) = extract_display_math(&expanded);

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let mut events: Vec<Event<'_>> = Vec::new();
    let mut language = String::new();
    let mut code = String::new();
    let mut in_code_block = false;

    for event in Parser::new_ext(&source, options) {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                language = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                code.clear();
                in_code_block = true;
            }
            Event::End(TagEnd::CodeBlock) if in_code_block => {
                in_code_block = false;
                // pulldown-cmark ends the last text run with the newline that
                // closes the fence; the highlighter wants the code alone.
                let code = code.strip_suffix('\n').unwrap_or(&code);
                let highlighted = (ctx.highlight)(&language, code);
                events.push(Event::Html(CowStr::Boxed(highlighted.into_boxed_str())));
            }
            Event::Text(text) if in_code_block => code.push_str(&text),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => events.push(Event::Start(Tag::Image {
                link_type,
                dest_url: CowStr::Boxed(post_asset_url(ctx.post_slug, &dest_url).into_boxed_str()),
                title,
                id,
            })),
            Event::Text(text) => {
                let protected = protect_inline_math(&text, &mut math);
                events.push(Event::Text(CowStr::Boxed(protected.into_boxed_str())));
            }
            other => events.push(other),
        }
    }

    let mut rendered = String::with_capacity(source.len());
    html::push_html(&mut rendered, events.into_iter());
    restore_math(&rendered, &math, ctx)
}

/// The math spans found in one body, in placeholder order.
struct MathStore {
    entries: Vec<(String, bool)>,
}

impl MathStore {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Record `latex` and return the placeholder that stands in for it.
    fn push(&mut self, latex: &str, display: bool) -> String {
        let kind = if display { DISPLAY_MATH } else { INLINE_MATH };
        let index = self.entries.len();
        self.entries.push((latex.to_string(), display));
        format!("{kind}{index}")
    }
}

/// Pull `$$...$$` spans out of the body before pulldown-cmark sees them.
///
/// A span is only taken when its body looks like TeX, so the prices and
/// exponents the posts are full of (`$0.078/M`, `$1/2-h`) stay text. Fenced
/// code is skipped: a `$$` in a code sample belongs to the sample.
fn extract_display_math(body: &str) -> (String, MathStore) {
    let mut store = MathStore::new();
    let mut out = String::with_capacity(body.len());
    let mut in_fence = false;

    for line in body.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            continue;
        }
        if in_fence {
            out.push_str(line);
            continue;
        }

        let mut cursor = 0;
        while let Some(open) = line[cursor..].find("$$").map(|at| cursor + at) {
            let body_start = open + 2;
            let Some(close) = line[body_start..].find("$$") else {
                break;
            };
            let latex = &line[body_start..body_start + close];
            if !looks_like_display_math(latex) {
                cursor = body_start;
                continue;
            }
            out.push_str(&line[cursor..open]);
            out.push_str(&store.push(latex.trim(), true));
            cursor = body_start + close + 2;
        }
        out.push_str(&line[cursor..]);
    }

    (out, store)
}

/// TeX display math always carries markup; a run of prose does not.
fn looks_like_display_math(latex: &str) -> bool {
    let trimmed = latex.trim();
    !trimmed.is_empty()
        && !latex.starts_with(char::is_whitespace)
        && !latex.ends_with(char::is_whitespace)
        && trimmed.contains(['\\', '^', '_', '{', '}'])
}

/// Swap `$...$` spans in a text run for placeholders.
fn protect_inline_math(text: &str, store: &mut MathStore) -> String {
    if !text.contains('$') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('$') {
        let body_start = open + 1;
        let Some(close) = rest[body_start..].find('$') else {
            break;
        };
        let latex = &rest[body_start..body_start + close];
        if !looks_like_inline_math(latex) {
            out.push_str(&rest[..body_start]);
            rest = &rest[body_start..];
            continue;
        }
        out.push_str(&rest[..open]);
        out.push_str(&store.push(latex, false));
        rest = &rest[body_start + close + 1..];
    }
    out.push_str(rest);
    out
}

/// Inline math is a tight span that does not open on a digit, which is what
/// separates `$u(x, t)$` from `$0.078/M`.
fn looks_like_inline_math(latex: &str) -> bool {
    if latex.is_empty() || latex.len() > 400 {
        return false;
    }
    let starts_with_digit = latex.starts_with(|ch: char| ch.is_ascii_digit());
    !starts_with_digit
        && !latex.starts_with(char::is_whitespace)
        && !latex.ends_with(char::is_whitespace)
        && !latex.contains('\n')
}

/// Replace the placeholders in the rendered HTML with the caller's math output.
fn restore_math(rendered: &str, store: &MathStore, ctx: &RenderCtx<'_>) -> String {
    let mut out = String::with_capacity(rendered.len());
    let mut chars = rendered.char_indices().peekable();
    while let Some((at, ch)) = chars.next() {
        if ch != INLINE_MATH && ch != DISPLAY_MATH {
            out.push(ch);
            continue;
        }
        let mut digits = String::new();
        while let Some(&(_, next)) = chars.peek() {
            if next.is_ascii_digit() {
                digits.push(next);
                chars.next();
            } else {
                break;
            }
        }
        match digits
            .parse::<usize>()
            .ok()
            .and_then(|index| store.entries.get(index))
        {
            Some((latex, display)) => out.push_str(&(ctx.render_math)(latex, *display)),
            None => {
                out.push_str(&rendered[at..at + ch.len_utf8()]);
                out.push_str(&digits);
            }
        }
    }
    out
}
/// A component attribute value: a quoted string, or a `{...}` JS expression.
enum AttrValue {
    Text(String),
    Expr(String),
}

/// The MDX components this site supports, in the order the scanner looks for
/// them. Anything else in the body is left exactly as written.
const COMPONENTS: [&str; 5] = [
    "Blockquote",
    "VideoEmbed",
    "PostTable",
    "PostImage",
    "Heading",
];

/// Rewrite the four MDX components into the HTML they rendered as, and drop
/// every `import` line.
///
/// Post-relative asset paths come out of here still relative; `render_markdown`
/// roots them at the post's own directory once it knows the slug.
pub fn expand_components(body: &str) -> String {
    let (imports, source) = strip_imports(body);
    let mut out = String::with_capacity(source.len());
    let mut in_fence = false;
    let mut offset = 0;

    while offset < source.len() {
        let line_end = source[offset..]
            .find('\n')
            .map_or(source.len(), |at| offset + at + 1);
        let line = &source[offset..line_end];

        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push_str(line);
            offset = line_end;
            continue;
        }

        if !in_fence && let Some((consumed, replacement)) = expand_at(&source[offset..], &imports) {
            out.push_str(&replacement);
            offset += consumed;
            continue;
        }

        out.push_str(line);
        offset = line_end;
    }

    translate_jsx_attributes(&out)
}

/// Split off the leading `import` lines and map each local identifier to the
/// path it was imported from.
fn strip_imports(body: &str) -> (BTreeMap<String, String>, String) {
    let mut imports = BTreeMap::new();
    let mut kept = String::with_capacity(body.len());
    let mut in_fence = false;

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            kept.push_str(line);
            continue;
        }
        if !in_fence && trimmed.starts_with("import ") {
            if let Some((name, path)) = parse_default_import(trimmed) {
                imports.insert(name, path);
            }
            continue;
        }
        kept.push_str(line);
    }

    (imports, kept)
}

/// `import Name from "./path";` -> `(Name, ./path)`.
fn parse_default_import(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("import ")?;
    let (name, rest) = rest.split_once(" from ")?;
    let name = name.trim();
    if name.is_empty() || name.contains(['{', '*']) {
        return None;
    }
    let path = rest.trim().trim_end_matches(';').trim();
    let path = path.strip_prefix('"')?.strip_suffix('"')?;
    Some((name.to_string(), path.to_string()))
}

/// Expand the component starting at `source` if there is one, returning how
/// many bytes it consumed and what to write in its place.
fn expand_at(source: &str, imports: &BTreeMap<String, String>) -> Option<(usize, String)> {
    let trimmed = source.trim_start();
    // Consumed counts from `source`, so the whitespace before the tag has to
    // come back out: blank lines around a component are layout, not content.
    let leading = source.len() - trimmed.len();
    if !trimmed.starts_with('<') {
        return None;
    }
    let name = COMPONENTS.iter().find(|name| {
        trimmed[1..].starts_with(**name)
            && !trimmed[1 + name.len()..].starts_with(|ch: char| ch.is_alphanumeric())
    })?;
    let (open_end, element_end) = scan_element(trimmed, name)?;
    let open_tag = &trimmed[1 + name.len()..open_end];
    // A self-closing tag has no children; anything else ends with `</Name>`.
    let inner = if element_end > open_end {
        &trimmed[open_end..element_end - (name.len() + 3)]
    } else {
        ""
    };
    let attrs = parse_attrs(open_tag);

    let rendered = match *name {
        "Blockquote" => render_blockquote(&attrs, inner),
        "VideoEmbed" => render_video_embed(&attrs)?,
        "PostTable" => render_post_table(&attrs)?,
        "PostImage" => render_post_image(&attrs, imports),
        "Heading" => render_heading(&attrs, inner),
        _ => return None,
    };
    Some((
        leading + element_end,
        format!("{}{}", &source[..leading], rendered),
    ))
}

/// Byte offsets of the end of an element's open tag and of the whole element.
fn scan_element(source: &str, name: &str) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut index = 1 + name.len();
    let mut quote: Option<u8> = None;
    let mut braces = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'{' => braces += 1,
            b'}' => braces = braces.saturating_sub(1),
            b'>' if braces == 0 => {
                let open_end = index + 1;
                if source[..index].trim_end().ends_with('/') {
                    return Some((open_end, open_end));
                }
                let close = format!("</{name}>");
                let at = source[open_end..].find(&close)?;
                return Some((open_end, open_end + at + close.len()));
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// Parse the attribute list of an open tag.
fn parse_attrs(open_tag: &str) -> Vec<(String, AttrValue)> {
    let mut attrs = Vec::new();
    let bytes = open_tag.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_ascii_whitespace() || bytes[index] == b'/') {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name = open_tag[name_start..index].to_string();
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != b'=' {
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        if index >= bytes.len() {
            break;
        }
        let value = match bytes[index] {
            b'"' | b'\'' => {
                let quote = bytes[index];
                let start = index + 1;
                index = start;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                AttrValue::Text(unescape(&open_tag[start..index.min(bytes.len())]))
            }
            b'{' => {
                let start = index + 1;
                let mut depth = 1usize;
                index = start;
                while index < bytes.len() && depth > 0 {
                    match bytes[index] {
                        b'{' => depth += 1,
                        b'}' => depth -= 1,
                        _ => {}
                    }
                    index += 1;
                }
                AttrValue::Expr(open_tag[start..index.saturating_sub(1)].trim().to_string())
            }
            _ => {
                let start = index;
                while index < bytes.len() && !bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                AttrValue::Text(open_tag[start..index].to_string())
            }
        };
        attrs.push((name, value));
    }

    attrs
}

/// Undo the backslash escapes a quoted attribute value may carry.
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn text_attr(attrs: &[(String, AttrValue)], name: &str) -> Option<String> {
    attrs.iter().find_map(|(key, value)| match value {
        AttrValue::Text(text) if key == name && !text.is_empty() => Some(text.clone()),
        _ => None,
    })
}

fn expr_attr<'a>(attrs: &'a [(String, AttrValue)], name: &str) -> Option<&'a str> {
    attrs.iter().find_map(|(key, value)| match value {
        AttrValue::Expr(expr) if key == name => Some(expr.as_str()),
        _ => None,
    })
}
/// `<Blockquote author source sourceUrl>text</Blockquote>`.
///
/// Mirrors `Blockquote.astro`: the quote body, then a footer carrying the
/// attribution with the source as a link when there is a URL and a `<cite>`
/// when there is not.
fn render_blockquote(attrs: &[(String, AttrValue)], inner: &str) -> String {
    let author = text_attr(attrs, "author");
    let source = text_attr(attrs, "source");
    let source_url = text_attr(attrs, "sourceUrl");
    let body = inner.trim();

    let mut children = vec![format!("<p>{}</p>", escape(body))];
    if author.is_some() || source.is_some() {
        let mut footer = String::from("\u{2014} ");
        if let Some(author) = &author {
            footer.push_str(&escape(author));
        }
        if author.is_some() && source.is_some() {
            footer.push_str(", ");
        }
        if let Some(source) = &source {
            footer.push_str(&match &source_url {
                Some(url) => el(
                    "a",
                    &[
                        ("href", url.as_str()),
                        ("target", "_blank"),
                        ("rel", "noopener noreferrer"),
                    ],
                    &[escape(source)],
                ),
                None => el("cite", &[], &[escape(source)]),
            });
        }
        children.push(el("footer", &[], &[footer]));
    }

    el("blockquote", &[("class", "quote")], &children)
}

/// `<VideoEmbed videoId url title />`, framed from the youtube-nocookie host.
fn render_video_embed(attrs: &[(String, AttrValue)]) -> Option<String> {
    let id = video_id(
        text_attr(attrs, "videoId").as_deref(),
        text_attr(attrs, "url").as_deref(),
    )?;
    let title = text_attr(attrs, "title").unwrap_or_else(|| "YouTube video".to_string());
    let src = format!("https://www.youtube-nocookie.com/embed/{id}");

    Some(el(
        "figure",
        &[("class", "video")],
        &[el(
            "div",
            &[("class", "video-frame")],
            &[el(
                "iframe",
                &[
                    ("src", src.as_str()),
                    ("title", title.as_str()),
                    ("loading", "lazy"),
                    (
                        "allow",
                        "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share",
                    ),
                    ("allowfullscreen", ""),
                    ("referrerpolicy", "strict-origin-when-cross-origin"),
                ],
                &[],
            )],
        )],
    ))
}

/// The id out of `videoId`, else out of a watch, short, or embed URL.
pub fn video_id(video_id: Option<&str>, url: Option<&str>) -> Option<String> {
    if let Some(id) = video_id.map(str::trim).filter(|id| !id.is_empty()) {
        return Some(id.to_string());
    }
    let url = url?;
    for marker in ["v=", "youtu.be/", "/embed/"] {
        if let Some(at) = url.find(marker) {
            let id: String = url[at + marker.len()..]
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
                .collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}
/// `<PostTable headers rows caption />`, matching `PostTable.astro`.
fn render_post_table(attrs: &[(String, AttrValue)]) -> Option<String> {
    let headers = parse_js_string_list(expr_attr(attrs, "headers")?);
    let rows = parse_js_string_grid(expr_attr(attrs, "rows")?);
    let caption = text_attr(attrs, "caption");

    let head_cells: Vec<String> = headers
        .iter()
        .map(|header| el("th", &[], &[escape(header)]))
        .collect();
    let head = el("thead", &[], &[el("tr", &[], &head_cells)]);

    let body_rows: Vec<String> = rows
        .iter()
        .map(|row| {
            let cells: Vec<String> = (0..headers.len())
                .map(|column| {
                    let cell = row.get(column).map_or("", String::as_str);
                    el("td", &[], &[escape(cell)])
                })
                .collect();
            el("tr", &[], &cells)
        })
        .collect();
    let body = el("tbody", &[], &body_rows);
    let table = el("table", &[], &[head, body]);

    let mut children = vec![el("div", &[("class", "table-scroll")], &[table])];
    if let Some(caption) = caption {
        children.push(el("figcaption", &[], &[escape(&caption)]));
    }
    Some(el("figure", &[], &children))
}

/// `<PostImage src alt caption />`, resolved through the import map.
fn render_post_image(attrs: &[(String, AttrValue)], imports: &BTreeMap<String, String>) -> String {
    let raw_src = expr_attr(attrs, "src")
        .and_then(|expr| imports.get(expr).cloned())
        .or_else(|| text_attr(attrs, "src"));
    let src = raw_src.map(|path| resolve_asset(&path)).unwrap_or_default();
    let alt = text_attr(attrs, "alt").unwrap_or_default();

    let mut children = vec![void_el(
        "img",
        &[
            ("src", src.as_str()),
            ("alt", alt.as_str()),
            ("loading", "lazy"),
            ("decoding", "async"),
        ],
    )];
    if let Some(caption) = text_attr(attrs, "caption") {
        children.push(el("figcaption", &[], &[escape(&caption)]));
    }
    el("figure", &[], &children)
}

/// Root a post-relative asset path at the post's own directory, slug known.
///
/// Markdown images in the posts are written relative to the post file
/// (`./images/image-0.png`); published, they live under the slug.
pub fn post_asset_url(slug: &str, path: &str) -> String {
    let path = path.trim();
    if path.starts_with('/') || path.starts_with("http") || path.starts_with("data:") {
        return path.to_string();
    }
    format!("/images/blog/{slug}/{}", strip_asset_prefix(path))
}

/// Root a post-relative asset path at the post's own directory.
///
/// `render_markdown` prefixes the post slug before parsing, so a path that is
/// already absolute is left alone.
fn resolve_asset(path: &str) -> String {
    let path = path.trim();
    if path.starts_with('/') || path.starts_with("http") {
        return path.to_string();
    }
    format!("{ASSET_ROOT}{}", strip_asset_prefix(path))
}

/// Drop the `./`, `../`, and `images/` prefixes a post writes its assets with.
fn strip_asset_prefix(path: &str) -> String {
    let mut stripped = path;
    while let Some(rest) = stripped.strip_prefix("./") {
        stripped = rest;
    }
    while let Some(rest) = stripped.strip_prefix("../") {
        stripped = rest;
    }
    // Posts keep their assets in `./images/`; the published tree does not
    // carry that segment, so the file sits directly under the slug.
    stripped
        .strip_prefix("images/")
        .unwrap_or(stripped)
        .to_string()
}

/// Where a post's own assets are published, before the slug is filled in.
const ASSET_ROOT: &str = "/images/blog/{slug}/";

/// Parse a JS string array literal: `["a", "b"]`.
pub fn parse_js_string_list(source: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '"' && ch != '\'' {
            continue;
        }
        let mut value = String::new();
        while let Some(next) = chars.next() {
            match next {
                '\\' => {
                    if let Some(escaped) = chars.next() {
                        value.push(match escaped {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                    }
                }
                closing if closing == ch => break,
                other => value.push(other),
            }
        }
        values.push(value);
    }
    values
}

/// Parse a JS array-of-arrays literal: `[["a", "b"], ["c", "d"]]`.
pub fn parse_js_string_grid(source: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut depth = 0usize;
    let mut start: Option<usize> = None;
    let mut chars = source.char_indices().peekable();

    while let Some((at, ch)) = chars.next() {
        match ch {
            '"' | '\'' => {
                while let Some((_, next)) = chars.next() {
                    if next == '\\' {
                        chars.next();
                    } else if next == ch {
                        break;
                    }
                }
            }
            '[' => {
                depth += 1;
                if depth == 2 {
                    start = Some(at);
                }
            }
            ']' => {
                if depth == 2
                    && let Some(begin) = start.take()
                {
                    rows.push(parse_js_string_list(&source[begin + 1..at]));
                }
                depth = depth.saturating_sub(1);
            }
            _ => {}
        }
    }
    rows
}
/// `<Heading as="h2">text</Heading>` from `Heading.astro`, reduced to the
/// element it renders.
///
/// MDX is gone, so the tag has to expand: left in place it would reach the
/// browser as an unknown element and the heading would lose its level.
fn render_heading(attrs: &[(String, AttrValue)], inner: &str) -> String {
    let level = text_attr(attrs, "as").unwrap_or_else(|| "h2".to_string());
    let tag = match level.as_str() {
        "h1" | "h3" | "h4" => level,
        _ => "h2".to_string(),
    };
    el(&tag, &[], &[escape(inner.trim())])
}

/// Translate the JSX-only attribute syntax MDX accepted and HTML does not:
/// `style={{ ... }}` becomes a declaration list and `className` becomes
/// `class`. Runs after the components expand, so it only ever sees the plain
/// HTML elements the posts write inline (terminal and status-line panels).
fn translate_jsx_attributes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some((tag_end, is_closing)) = scan_tag(tail) else {
            out.push('<');
            rest = &tail[1..];
            continue;
        };
        let tag = &tail[..tag_end];
        if is_closing || !(tag.contains("={") || tag.contains("className=")) {
            out.push_str(tag);
        } else {
            out.push_str(&rewrite_tag(tag));
        }
        rest = &tail[tag_end..];
    }
    out.push_str(rest);
    out
}

/// Byte offset just past the `>` that ends the tag at `source`, and whether
/// it was a closing tag. `None` when this `<` does not start a tag.
fn scan_tag(source: &str) -> Option<(usize, bool)> {
    let bytes = source.as_bytes();
    if bytes.len() < 3 || !(bytes[1].is_ascii_alphabetic() || bytes[1] == b'/') {
        return None;
    }
    let mut index = 1;
    let mut quote: Option<u8> = None;
    let mut braces = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if byte == b'\\' {
                index += 2;
                continue;
            }
            if byte == open {
                quote = None;
            }
        } else {
            match byte {
                b'"' | b'\'' => quote = Some(byte),
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                b'>' if braces == 0 => {
                    let is_closing = source[1..].starts_with('/');
                    return Some((index + 1, is_closing));
                }
                b'<' => return None,
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// Rebuild one open tag with its attributes translated to HTML.
fn rewrite_tag(tag: &str) -> String {
    let name_end = tag[1..]
        .find(|ch: char| !ch.is_ascii_alphanumeric())
        .map_or(tag.len(), |at| at + 1);
    let name = &tag[1..name_end];
    let self_closing = tag.trim_end().ends_with("/>");

    let mut out = format!("<{name}");
    for (key, value) in parse_attrs(&tag[name_end..]) {
        let key = if key == "className" {
            "class".to_string()
        } else {
            key
        };
        match value {
            AttrValue::Text(text) => {
                out.push_str(&format!(" {key}=\"{}\"", escape(&text)));
            }
            AttrValue::Expr(expr) => {
                if key == "style" {
                    let css = jsx_object_to_css(&expr);
                    if !css.is_empty() {
                        out.push_str(&format!(" style=\"{}\"", escape(&css)));
                    }
                } else if let Some(literal) = js_string_literal(&expr) {
                    out.push_str(&format!(" {key}=\"{}\"", escape(&literal)));
                }
                // An expression HTML cannot carry is dropped rather than leaked.
            }
        }
    }
    if self_closing {
        out.push_str(" />");
    } else {
        out.push('>');
    }
    out
}

/// `{ color: "#fff", fontSize: "0.7rem" }` -> `color: #fff; font-size: 0.7rem`.
fn jsx_object_to_css(expr: &str) -> String {
    let inner = expr.trim().trim_start_matches('{').trim_end_matches('}');
    let mut declarations = Vec::new();
    for entry in split_top_level(inner, ',') {
        let Some((key, value)) = entry.split_once(':') else {
            continue;
        };
        let key = kebab_case(key.trim());
        let value = value.trim().trim_matches(['"', '\'']);
        if key.is_empty() || value.is_empty() {
            continue;
        }
        declarations.push(format!("{key}: {value}"));
    }
    declarations.join("; ")
}

/// Split on `sep` at the top level only, so commas inside strings, calls, and
/// nested braces stay put.
fn split_top_level(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    let mut quote: Option<char> = None;
    for (at, ch) in text.char_indices() {
        if let Some(open) = quote {
            if ch == open {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth = depth.saturating_sub(1),
            other if other == sep && depth == 0 => {
                parts.push(&text[start..at]);
                start = at + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

/// `fontSize` -> `font-size`.
fn kebab_case(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for ch in key.chars() {
        if ch.is_ascii_uppercase() {
            out.push('-');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// The inner text of a JS string literal expression, if that is what it is.
fn js_string_literal(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    let quote = trimmed.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    trimmed
        .strip_prefix(quote)?
        .strip_suffix(quote)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(
        h: &'a dyn Fn(&str, &str) -> String,
        m: &'a dyn Fn(&str, bool) -> String,
    ) -> RenderCtx<'a> {
        RenderCtx {
            post_slug: "demo",
            highlight: h,
            render_math: m,
        }
    }

    fn no_highlight(_lang: &str, code: &str) -> String {
        format!("<pre><code>{code}</code></pre>")
    }

    fn no_math(tex: &str, display: bool) -> String {
        format!("<math display=\"{display}\">{tex}</math>")
    }

    #[test]
    fn renders_headings_paragraphs_and_links() {
        let out = render_markdown(
            "# Title\n\nBody with [a link](/x).",
            &ctx(&no_highlight, &no_math),
        );
        assert!(out.contains("<h1>Title</h1>"), "heading: {out}");
        assert!(out.contains("<a href=\"/x\">a link</a>"), "link: {out}");
    }

    #[test]
    fn renders_gfm_tables() {
        let out = render_markdown(
            "| a | b |\n| - | - |\n| 1 | 2 |",
            &ctx(&no_highlight, &no_math),
        );
        assert!(out.contains("<table>"), "table: {out}");
        assert!(out.contains("<td>1</td>"), "cell: {out}");
    }

    #[test]
    fn expands_blockquote_with_attribution() {
        let body = r#"<Blockquote author="Ryan" source="dev.to" sourceUrl="https://dev.to/x">Quote text.</Blockquote>"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("<blockquote class=\"quote\">"), "class: {out}");
        assert!(out.contains("Quote text."), "text: {out}");
        assert!(out.contains("href=\"https://dev.to/x\""), "href: {out}");
        assert!(out.contains("\u{2014} Ryan, "), "attribution: {out}");
        assert!(out.contains(">dev.to</a>"), "source link text: {out}");
    }

    #[test]
    fn expands_video_embed_from_url() {
        let body = r#"<VideoEmbed videoId="lwz2vk28ROA" url="https://www.youtube.com/watch?v=lwz2vk28ROA" title="A talk" />"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(
            out.contains("https://www.youtube-nocookie.com/embed/lwz2vk28ROA"),
            "embed src: {out}"
        );
        assert!(out.contains("title=\"A talk\""), "title: {out}");
    }

    #[test]
    fn derives_video_id_from_short_and_embed_urls() {
        assert_eq!(
            video_id(None, Some("https://youtu.be/abc12345678")),
            Some("abc12345678".into())
        );
        assert_eq!(
            video_id(None, Some("https://www.youtube.com/embed/abc12345678")),
            Some("abc12345678".into())
        );
        assert_eq!(video_id(None, Some("https://example.com")), None);
    }

    #[test]
    fn expands_post_table() {
        let body = r#"<PostTable headers={["Window", "Nudge"]} rows={[["200K", "70%"]]} caption="Cap." />"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("<th>Window</th>"), "header: {out}");
        assert!(out.contains("<td>70%</td>"), "cell: {out}");
        assert!(out.contains("Cap."), "caption: {out}");
    }

    #[test]
    fn resolves_post_image_through_the_import_map() {
        let body = "import Ratchet from \"./images/abstraction-ratchet.svg\";\n\n<PostImage src={Ratchet} alt=\"A chart\" caption=\"Fig 1\" />";
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(
            out.contains("/images/blog/demo/abstraction-ratchet.svg"),
            "asset path: {out}"
        );
        assert!(out.contains("alt=\"A chart\""), "alt: {out}");
        assert!(!out.contains("import "), "import survived: {out}");
    }

    #[test]
    fn leaves_unknown_tags_alone() {
        let out = render_markdown("<div>hi</div>", &ctx(&no_highlight, &no_math));
        assert!(out.contains("<div>hi</div>"), "unknown tag: {out}");
    }

    #[test]
    fn routes_fenced_code_through_the_highlighter() {
        let out = render_markdown("```rust\nfn main() {}\n```", &ctx(&no_highlight, &no_math));
        assert!(
            out.contains("<pre><code>fn main() {}</code></pre>"),
            "highlight hook: {out}"
        );
    }

    #[test]
    fn renders_inline_and_display_math_through_the_hook() {
        let out = render_markdown(
            "Let $x^2$ be, and\n\n$$\\nabla \\cdot u = 0$$\n",
            &ctx(&no_highlight, &no_math),
        );
        assert!(
            out.contains("<math display=\"false\">x^2</math>"),
            "inline math: {out}"
        );
        assert!(
            out.contains("<math display=\"true\">\\nabla \\cdot u = 0</math>"),
            "display math: {out}"
        );
    }

    #[test]
    fn expands_a_multi_line_post_table() {
        let body = "<PostTable\n  headers={[\"Window\", \"Nudge\"]}\n  rows={[\n    [\"200K\", \"70%\"],\n    [\"1M\", \"50%\"],\n]}\n  caption=\"Cap.\"\n/>";
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("<th>Window</th>"), "header: {out}");
        assert!(out.contains("<td>1M</td>"), "second row: {out}");
        assert!(
            out.contains("<figcaption>Cap.</figcaption>"),
            "caption: {out}"
        );
    }

    #[test]
    fn renders_a_source_without_a_url_as_cite() {
        let body = r#"<Blockquote author="Ryan" source="dev.to">Quote text.</Blockquote>"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("<cite>dev.to</cite>"), "cite: {out}");
    }

    #[test]
    fn leaves_components_and_imports_inside_code_fences_alone() {
        let body = "```mdx\nimport X from \"./x.svg\";\n<PostImage src={X} alt=\"a\" />\n```";
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(
            out.contains("import X from"),
            "import stripped from a fence: {out}"
        );
        assert!(
            !out.contains("<figure>"),
            "fenced component expanded: {out}"
        );
    }

    #[test]
    fn leaves_currency_amounts_as_text() {
        let out = render_markdown(
            "Costs $0.078/M today and the $1/2-h exponent tomorrow.",
            &ctx(&no_highlight, &no_math),
        );
        assert!(!out.contains("<math"), "currency became math: {out}");
        assert!(out.contains("$0.078/M"), "currency text lost: {out}");
    }

    #[test]
    fn renders_alt_text_when_a_post_image_has_no_import() {
        let body = r#"<PostImage src={Missing} alt="A chart" />"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("A chart"), "alt lost: {out}");
    }

    #[test]
    fn expands_heading_to_its_level() {
        let body = r#"<Heading as="h3">The Insight</Heading>"#;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("<h3>The Insight</h3>"), "heading: {out}");
    }

    #[test]
    fn translates_a_jsx_style_object_to_css() {
        let body = r##"<div style={{ backgroundColor: "#011627", fontSize: "0.7rem" }}>hi</div>"##;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(
            out.contains("style=\"background-color: #011627; font-size: 0.7rem\""),
            "style translation: {out}"
        );
        assert!(!out.contains("{"), "jsx braces survived: {out}");
        assert!(out.contains(">hi</div>"), "element lost: {out}");
    }

    #[test]
    fn translates_class_name_and_nested_expressions() {
        let body = r##"<div className="panel" style={{ marginTop: "0.5rem" }}><span style={{ color: "#ff5874" }}>x</span></div>"##;
        let out = render_markdown(body, &ctx(&no_highlight, &no_math));
        assert!(out.contains("class=\"panel\""), "className: {out}");
        assert!(
            out.contains("style=\"margin-top: 0.5rem\""),
            "outer style: {out}"
        );
        assert!(
            out.contains("style=\"color: #ff5874\""),
            "nested style: {out}"
        );
    }
}
