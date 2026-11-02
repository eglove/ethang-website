//! Build-time syntax highlighting with syntect and a Night Owl theme.
//!
//! Highlighting happens once, at build time, so the site ships no highlighter:
//! every fence in a post becomes `<pre class="code"><code>` plus a run of
//! `<span style="color:#rrggbb">` segments, with no script to run afterwards.
//!
//! The theme is a TextMate plist embedded with `include_str!`, so the palette is
//! readable and editable next to the rest of the assets. Building the syntax set
//! and parsing the theme is slow enough that it happens once per process, in
//! [`highlighter`], rather than once per fence.

use crate::html::escape;
use std::fmt::Write as _;
use std::io::Cursor;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, Style, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

/// The Night Owl theme, kept as a plist asset rather than a Rust literal so the
/// colours stay diffable against the old `shikiConfig: { theme: "night-owl" }`.
const THEME_PLIST: &str = include_str!("../assets/night-owl.tmTheme");

/// The prefix every highlighted block starts with, and its closing counterpart.
const OPEN: &str = "<pre class=\"code\"><code>";
const CLOSE: &str = "</code></pre>";

/// A syntax set plus a theme: everything needed to colour one block of code.
pub struct HighlightEngine {
    syntaxes: SyntaxSet,
    theme: Theme,
}

static ENGINE: OnceLock<HighlightEngine> = OnceLock::new();

/// The process-wide engine. The first call builds it; every later call reuses it.
pub fn highlighter() -> &'static HighlightEngine {
    ENGINE.get_or_init(HighlightEngine::new)
}

/// Highlight `code` as `lang` and return the complete `<pre><code>` block.
///
/// The result is raw HTML: callers push it through unescaped. An unknown or
/// empty language degrades to plain escaped text, never to an error, so a fence
/// with a language this build does not know still renders.
pub fn highlight(lang: &str, code: &str) -> String {
    highlighter().render(lang, code)
}

impl HighlightEngine {
    fn new() -> Self {
        // `two-face` is syntect's default set plus the grammars bat carries:
        // the extras include TypeScript and TOML, which the posts use and the
        // default set does not know.
        let syntaxes = two_face::syntax::extra_newlines();
        let theme = ThemeSet::load_from_reader(&mut Cursor::new(THEME_PLIST.as_bytes()))
            .expect("the embedded Night Owl theme parses");
        HighlightEngine { syntaxes, theme }
    }

    /// Render one block. `lang` is the fence info string, already reduced to its
    /// first word by the markdown renderer.
    pub fn render(&self, lang: &str, code: &str) -> String {
        let mut segments = Vec::new();
        if !self.collect(self.syntax(lang), code, &mut segments) {
            // A grammar that fails mid-block would otherwise colour half a fence.
            // Drop the partial run and redo the whole block as plain text.
            segments.clear();
            self.collect(self.syntaxes.find_syntax_plain_text(), code, &mut segments);
        }

        let mut out = String::with_capacity(code.len() * 2 + OPEN.len() + CLOSE.len());
        out.push_str(OPEN);
        for (css, text) in merge(segments) {
            let _ = write!(out, "<span style=\"{css}\">{}</span>", escape(&text));
        }
        out.push_str(CLOSE);
        out
    }

    /// Walk `code` line by line, appending `(style, text)` runs to `out`.
    /// Returns false if the grammar gave up, leaving `out` incomplete.
    fn collect(
        &self,
        syntax: &SyntaxReference,
        code: &str,
        out: &mut Vec<(Style, String)>,
    ) -> bool {
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        for line in LinesWithEndings::from(code) {
            let Ok(segments) = highlighter.highlight_line(line, &self.syntaxes) else {
                return false;
            };
            for (style, text) in segments {
                if !text.is_empty() {
                    out.push((style, text.to_string()));
                }
            }
        }
        true
    }

    /// Resolve a fence language to a grammar, falling back to plain text.
    fn syntax(&self, lang: &str) -> &SyntaxReference {
        let token = lang.trim();
        if token.is_empty() {
            return self.syntaxes.find_syntax_plain_text();
        }
        let found = match extension_for(token) {
            // An empty extension means "this fence is deliberately plain": prose
            // blocks, and mermaid diagrams, which the math/mermaid pass replaces
            // with committed SVG. Colouring a diagram's source would only ever
            // show up if that pass has not run, so leave it as escaped text.
            Some("") => None,
            Some(extension) => self.syntaxes.find_syntax_by_extension(extension),
            None => self.syntaxes.find_syntax_by_token(token),
        };
        found.unwrap_or_else(|| self.syntaxes.find_syntax_plain_text())
    }
}

/// Map the language names the posts use onto syntect's file extensions.
///
/// The posts were written for shiki, which accepts names like `csharp` that are
/// not file extensions; syntect matches extensions and syntax names, so the
/// aliases shiki accepts but syntect does not are listed here. A `None` means
/// "let syntect try the token itself", which covers the names that already
/// resolve, such as `json` and `rust`.
fn extension_for(lang: &str) -> Option<&'static str> {
    let ext = match lang.to_ascii_lowercase().as_str() {
        // Plain prose, and diagrams owned by the math/mermaid pass.
        "text" | "txt" | "plaintext" | "plain" | "mermaid" => "",
        "typescript" | "ts" | "tsx" => "ts",
        "javascript" | "js" | "jsx" => "js",
        "csharp" | "c#" | "cs" => "cs",
        "shell" | "sh" | "bash" | "zsh" | "console" => "sh",
        "yaml" | "yml" => "yaml",
        "markdown" | "md" => "md",
        "html" | "htm" => "html",
        "json" | "jsonc" => "json",
        "rust" | "rs" => "rs",
        _ => return None,
    };
    Some(ext)
}

/// Collapse the run list into `(css, text)` pairs, joining neighbours that share
/// a style so a plain word is not split across three identical spans.
///
/// Every pair carries a foreground colour, because the theme always resolves
/// one: an unstyled run gets the theme's `foreground`, not a missing style.
fn merge(segments: Vec<(Style, String)>) -> Vec<(String, String)> {
    let mut runs: Vec<(String, String)> = Vec::new();
    for (style, text) in segments {
        let css = css_for(style);
        match runs.last_mut() {
            Some((last, buffer)) if *last == css => buffer.push_str(&text),
            _ => runs.push((css, text)),
        }
    }
    runs
}

/// The inline style for one run. Only the foreground colour and font style are
/// emitted; the block background comes from the stylesheet.
fn css_for(style: Style) -> String {
    let foreground = style.foreground;
    let mut css = format!(
        "color:#{:02x}{:02x}{:02x}",
        foreground.r, foreground.g, foreground.b
    );
    if style.font_style.contains(FontStyle::ITALIC) {
        css.push_str(";font-style:italic");
    }
    if style.font_style.contains(FontStyle::BOLD) {
        css.push_str(";font-weight:bold");
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        css.push_str(";text-decoration:underline");
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_code_in_a_pre() {
        let out = highlight("rust", "fn main() {}");
        assert!(out.starts_with("<pre class=\"code\"><code>"));
        assert!(out.ends_with("</code></pre>"));
    }

    #[test]
    fn colours_keywords_with_the_night_owl_palette() {
        let out = highlight("rust", "fn main() {}");
        assert!(
            out.contains("color:#c792ea"),
            "keyword colour missing: {out}"
        );
    }

    #[test]
    fn escapes_html_in_code() {
        let out = highlight("html", "<div>&</div>");
        assert!(out.contains("&lt;div&gt;"));
        assert!(!out.contains("<div>"));
    }

    #[test]
    fn unknown_language_falls_back_to_plain_text() {
        let out = highlight("not-a-language", "hello");
        assert!(out.contains("hello"));
    }

    #[test]
    fn highlights_the_languages_the_posts_use() {
        // The fence languages in `content/blog`, each expected to carry at least
        // one colour beyond the default foreground.
        let cases = [
            ("typescript", "const x: number = 1;"),
            ("json", "{\"a\": 1}"),
            ("csharp", "public class Foo {}"),
            ("html", "<div class=\"x\"></div>"),
            ("yaml", "name: ethang"),
            ("markdown", "# Title"),
        ];
        for (lang, code) in cases {
            let out = highlight(lang, code);
            let colours: std::collections::BTreeSet<&str> = out
                .match_indices("color:#")
                .map(|(i, _)| &out[i + 6..i + 13])
                .collect();
            assert!(colours.len() > 1, "{lang} produced only {colours:?}: {out}");
        }
    }

    #[test]
    fn typescript_keywords_and_types_take_distinct_colours() {
        let out = highlight("typescript", "const name: string = 'x';");
        assert!(out.contains("color:#c792ea"), "keyword: {out}");
        assert!(out.contains("color:#ffcb8b"), "type: {out}");
    }

    #[test]
    fn mermaid_is_left_as_plain_text() {
        // Task 6 replaces mermaid fences with committed SVG. Until then the source
        // has to survive as readable, uncoloured text rather than as a guess.
        let out = highlight("mermaid", "graph TD; A-->B;");
        assert!(out.contains("graph TD; A--&gt;B;"));
        assert!(
            !out.contains("color:#c792ea"),
            "mermaid was coloured: {out}"
        );
    }

    #[test]
    fn an_empty_language_is_plain_text() {
        let out = highlight("", "just prose");
        assert!(out.contains("just prose"));
    }

    #[test]
    fn keeps_line_structure() {
        let out = highlight("rust", "let a = 1;\nlet b = 2;");
        assert!(out.contains('\n'), "newlines lost: {out}");
        assert_eq!(out.matches('\n').count(), 1, "newline count: {out}");
    }

    #[test]
    fn every_block_is_well_formed() {
        // Well-formed means no stray close and no unclosed span: a depth counter
        // that never goes negative and lands back on zero. A span may straddle a
        // newline, which is legal and is what keeps the line breaks intact.
        let out = highlight("typescript", "const a = `x${1}y`;\n// done\n");
        let body = out.strip_prefix(OPEN).and_then(|s| s.strip_suffix(CLOSE));
        let body = body.expect("block is wrapped in the pre/code pair");
        let mut depth = 0i32;
        let mut rest = body;
        while let Some(index) = rest.find('<') {
            rest = &rest[index..];
            if rest.starts_with("<span ") {
                depth += 1;
                rest = &rest[1..];
            } else if rest.starts_with("</span>") {
                depth -= 1;
                assert!(depth >= 0, "stray closing span in: {body}");
                rest = &rest[1..];
            } else {
                panic!("unexpected markup inside a code block: {rest}");
            }
        }
        assert_eq!(depth, 0, "unclosed span in: {body}");
    }

    #[test]
    fn the_engine_is_built_once() {
        let first = highlighter() as *const HighlightEngine;
        let second = highlighter() as *const HighlightEngine;
        assert_eq!(first, second);
    }
}
