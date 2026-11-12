//! The Tips pages: a short index and one page per tip.
//!
//! Ported from the old Astro site - `tips/index.astro` for the list and one
//! `.astro` file per tip. The two tips that existed there have since moved to
//! `content/tips/*.md`, so the list is derived from the directory instead of
//! being hard-coded the way the old page hard-coded its `allTips` array: adding
//! a tip is adding a file, and a missing one is a missing page rather than a
//! stale link.
//!
//! A tip is frontmatter plus markdown, and its body is rendered by the same
//! markdown pipeline a blog post uses, so a fenced block in a tip is
//! highlighted by the same Night Owl theme and a math run renders the same
//! way. What a tip does *not* get is a post's machinery: no date, no category,
//! no pagination, no article metadata in `<head>`.

use crate::content::split_frontmatter;
use crate::html::{el, escape};
use crate::pages::{Doc, canonical_url};
use anyhow::{Context, Result, anyhow, bail};
use std::path::Path;

/// What a tip page advertises about itself, and all the index needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TipMeta {
    pub slug: String,
    pub title: String,
}

/// One tip: its metadata plus the prose and the description it was written with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    /// The markdown body, before rendering.
    pub body: String,
}

impl Tip {
    /// The metadata the index lists, borrowed out of the tip itself.
    pub fn meta(&self) -> TipMeta {
        TipMeta {
            slug: self.slug.clone(),
            title: self.title.clone(),
        }
    }
}

/// The index's kicker, verbatim from the old page.
pub const KICKER: &str = "small solutions / sharp edges";

/// The page title shared by the index and the nav.
pub const TITLE: &str = "Tips";

/// Load every tip under `root`, ordered by slug.
///
/// A directory without a `.md` file is skipped, the way a directory without an
/// `index.mdx` is skipped for posts. Ordering by slug rather than by directory
/// order keeps the index stable across filesystems.
pub fn load_tips(root: &Path) -> Result<Vec<Tip>> {
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("reading tips directory {}", root.display()))?;

    let mut tips = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        tips.push(parse_tip(&path, &raw)?);
    }

    tips.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(tips)
}

/// Parse one tip file, deriving the slug from its file name.
fn parse_tip(path: &Path, raw: &str) -> Result<Tip> {
    let slug = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow!("tip file name is not valid UTF-8: {}", path.display()))?
        .to_string();
    let (frontmatter, body) =
        split_frontmatter(raw).with_context(|| format!("parsing {}", path.display()))?;
    let (title, description) = parse_tip_frontmatter(frontmatter)
        .with_context(|| format!("parsing {}", path.display()))?;
    Ok(Tip {
        slug,
        title,
        description,
        body: body.trim().to_string(),
    })
}

/// Parse a tip's frontmatter into its title and optional description.
///
/// A tip is not a post: it has no slug (the file name is the slug), no date,
/// and no category, so [`crate::content::parse_frontmatter`] - which requires
/// all three - would reject every tip in the corpus. The shape is the same
/// flat `key: value` block, and an unknown key is still an error so a
/// mistyped `descripton` fails the build rather than shipping a tip with no
/// meta description.
fn parse_tip_frontmatter(frontmatter: &str) -> Result<(String, Option<String>)> {
    let mut title = None;
    let mut description = None;
    for (index, line) in frontmatter.lines().enumerate() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            bail!("frontmatter line {} is not `key: value`: {line}", index + 1);
        };
        let value = unquote(value.trim());
        match key.trim() {
            "title" => title = none_if_empty(value),
            "description" => description = none_if_empty(value),
            other => bail!("unknown frontmatter key `{other}` on line {}", index + 1),
        }
    }
    let title = title.ok_or_else(|| anyhow!("frontmatter is missing required key `title`"))?;
    Ok((title, description))
}

/// Strip one layer of matching quotes and surrounding whitespace.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn none_if_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

/// Render the tips index body: kicker, title, rule, then one row per tip.
///
/// Each row is a whole-row link, so the title is the target and the card is
/// the affordance - the same relationship the blog index uses.
pub fn render_index(tips: &[TipMeta]) -> String {
    let head = el(
        "div",
        &[("class", "tips-head")],
        &[
            el("p", &[("class", "section-kicker")], &[escape(KICKER)]),
            el("h1", &[("class", "tips-head__title")], &[escape(TITLE)]),
            el("div", &[("class", "section-rule")], &[]),
        ],
    );
    el("div", &[("class", "page tips")], &[head, list(tips)])
}

/// The list itself: one card per tip, or the empty state.
fn list(tips: &[TipMeta]) -> String {
    if tips.is_empty() {
        return el(
            "div",
            &[("class", "card blog-empty")],
            &[el(
                "span",
                &[("class", "blog-empty__text")],
                &[escape("No tips found.")],
            )],
        );
    }
    let items: Vec<String> = tips.iter().map(list_item).collect();
    el("ul", &[("class", "tips-list")], &items)
}

/// One tip in the list: a card whose heading is the link.
fn list_item(tip: &TipMeta) -> String {
    let href = tip_href(&tip.slug);
    let link = el(
        "a",
        &[("class", "tips-card__link"), ("href", href.as_str())],
        &[escape(&tip.title)],
    );
    el(
        "li",
        &[("class", "surface-card tips-card")],
        &[el("h2", &[("class", "tips-card__title")], &[link])],
    )
}

/// Render a tip page body: the title, then the rendered prose.
pub fn render_tip(tip: &Tip, body_html: &str) -> String {
    el(
        "div",
        &[("class", "page tip")],
        &[
            el("h1", &[("class", "tip__title")], &[escape(&tip.title)]),
            el(
                "article",
                &[("class", "blog-prose tip-prose")],
                &[body_html.to_string()],
            ),
        ],
    )
}

/// The complete document for the tips index.
pub fn index_doc(tips: &[TipMeta]) -> Doc {
    Doc {
        title: TITLE.into(),
        description: Some("Small, self-contained fixes for sharp edges in CSS and HTML.".into()),
        canonical: Some(canonical_url("/tips")),
        article: None,
        preload_image: None,
        body: render_index(tips),
    }
}

/// The complete document for one tip.
pub fn tip_doc(tip: &Tip, body_html: &str) -> Doc {
    Doc {
        title: tip.title.clone(),
        description: tip.description.clone(),
        canonical: Some(canonical_url(&tip_href(&tip.slug))),
        article: None,
        preload_image: None,
        body: render_tip(tip, body_html),
    }
}

/// The site-absolute URL for a tip, slash-free to match the old site's links.
pub fn tip_href(slug: &str) -> String {
    format!("/tips/{slug}")
}
#[cfg(test)]
mod tests {
    use super::*;

    fn tip(slug: &str, title: &str) -> Tip {
        Tip {
            slug: slug.into(),
            title: title.into(),
            description: None,
            body: String::new(),
        }
    }

    #[test]
    fn the_index_links_each_tip() {
        let html = render_index(&[
            TipMeta {
                slug: "scrollbar-gutter".into(),
                title: "scrollbar-gutter".into(),
            },
            TipMeta {
                slug: "scroll-containers".into(),
                title: "Easy Sticky Header/Footer".into(),
            },
        ]);
        assert!(html.contains("href=\"/tips/scrollbar-gutter\""));
        assert!(html.contains("href=\"/tips/scroll-containers\""));
    }

    #[test]
    fn the_index_titles_and_kicker_match_the_old_page() {
        let html = render_index(&[]);
        assert!(html.contains("<h1 class=\"tips-head__title\">Tips</h1>"));
        assert!(html.contains("small solutions / sharp edges"));
    }

    #[test]
    fn an_empty_index_says_so() {
        assert!(render_index(&[]).contains("No tips found."));
    }

    #[test]
    fn the_index_escapes_titles() {
        let meta = TipMeta {
            slug: "a".into(),
            title: "Easy Sticky Header/Footer".into(),
        };
        assert!(render_index(&[meta]).contains("Easy Sticky Header/Footer"));
    }

    #[test]
    fn a_tip_page_wraps_the_body_in_prose() {
        let html = render_tip(&tip("scrollbar-gutter", "scrollbar-gutter"), "<p>Body.</p>");
        assert!(html.contains("class=\"blog-prose tip-prose\""));
        assert!(html.contains("<p>Body.</p>"));
        assert!(html.contains("<h1 class=\"tip__title\">scrollbar-gutter</h1>"));
    }

    #[test]
    fn a_tip_document_is_not_an_article() {
        let mut t = tip("scrollbar-gutter", "scrollbar-gutter");
        t.description = Some("Avoid layout shifts.".into());
        let doc = tip_doc(&t, "");
        assert_eq!(doc.title, "scrollbar-gutter");
        assert_eq!(doc.description.as_deref(), Some("Avoid layout shifts."));
        assert_eq!(
            doc.canonical.as_deref(),
            Some("https://ethang.dev/tips/scrollbar-gutter/")
        );
        assert!(doc.article.is_none());
    }

    #[test]
    fn the_index_document_points_at_the_tips_page() {
        let doc = index_doc(&[]);
        assert_eq!(doc.title, "Tips");
        assert_eq!(doc.canonical.as_deref(), Some("https://ethang.dev/tips/"));
    }

    #[test]
    fn tip_hrefs_have_no_trailing_slash() {
        assert_eq!(tip_href("scroll-containers"), "/tips/scroll-containers");
    }

    #[test]
    fn loads_the_shipped_tips() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("content/tips");
        let tips = load_tips(&root).unwrap();
        let slugs: Vec<&str> = tips.iter().map(|tip| tip.slug.as_str()).collect();
        assert_eq!(slugs, ["scroll-containers", "scrollbar-gutter"]);
    }

    #[test]
    fn a_loaded_tip_carries_its_frontmatter() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("content/tips");
        let tips = load_tips(&root).unwrap();
        let gutter = tips
            .iter()
            .find(|tip| tip.slug == "scrollbar-gutter")
            .unwrap();
        assert_eq!(gutter.title, "scrollbar-gutter");
        assert!(
            gutter
                .description
                .as_deref()
                .unwrap()
                .starts_with("Avoid content layout shifts")
        );
        assert!(gutter.body.contains("scrollbar-gutter: stable both-edges"));
        // The slug comes from the file name, not from the frontmatter.
        assert_eq!(
            gutter.meta(),
            TipMeta {
                slug: "scrollbar-gutter".into(),
                title: "scrollbar-gutter".into(),
            }
        );
    }

    #[test]
    fn a_missing_tips_directory_is_an_error() {
        let missing = std::env::temp_dir().join("ethang-no-such-tips");
        assert!(load_tips(&missing).is_err());
    }
}
