//! `llms.txt` and `llms-full.txt`: the site as an agent reads it.
//!
//! The proposal at <https://llmstxt.org/> asks for a markdown file at the site
//! root that names the site, summarises it, and links to LLM-friendly versions
//! of its pages. Every page here therefore has a markdown twin at the same
//! path with `.md` appended to its directory (`/blog/alpha/index.md`), and the
//! two files this module renders are the index of those twins:
//!
//! - `llms.txt` is the index alone - small enough to sit in a context window,
//!   with the detail behind its links.
//! - `llms-full.txt` is the same document with every body inlined, for a reader
//!   that would rather fetch once than follow links.
//!
//! Both are generated from the same [`Document`] list the pages are written
//! from, so a page cannot be published without appearing here, and a link here
//! cannot point at a file that was not written.

use crate::html::escape;
use crate::pages::{SITE, canonical_url};

/// One page, as both files describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// Which list the page belongs to: `Pages`, `Writing`, or `Tips`.
    pub section: Section,
    pub title: String,
    /// Site path of the markdown twin, e.g. `/blog/alpha/index.md`.
    pub path: String,
    /// One line about what the page is.
    pub note: String,
    /// The page as markdown, for `llms-full.txt`.
    pub body: String,
}

/// The sections of the index, in the order they are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Pages,
    Writing,
    Tips,
}

impl Section {
    /// The `##` heading this section is filed under.
    pub fn heading(self) -> &'static str {
        match self {
            Section::Pages => "Pages",
            Section::Writing => "Writing",
            Section::Tips => "Tips",
        }
    }

    /// The order the spec's example puts sections in: what the site is first,
    /// then what it publishes.
    fn order(self) -> u8 {
        match self {
            Section::Pages => 0,
            Section::Writing => 1,
            Section::Tips => 2,
        }
    }
}

/// The site's one-line summary, used by both files.
pub const SUMMARY: &str = "Ethan Glover's site: notes on software engineering, domain-driven design, and the systems behind them, plus a curriculum for people who write software with AI assistance.";

/// The guidance block between the summary and the file lists.
///
/// The spec allows any non-heading markdown here, and this is the one place a
/// reader learns that the markdown twins exist and what they contain.
const GUIDANCE: &str = "Every page on this site has a markdown version at the same path with `.md` appended to its directory: `/blog/<slug>/index.md` is the markdown of `/blog/<slug>/`, and `/index.md` is the markdown of the home page. The HTML pages carry no JavaScript, inline all of their CSS, and are minified. `llms-full.txt` at the site root is this file with every page's body inlined.";

/// The absolute URL of a markdown twin, for a link in either file.
pub fn absolute(path: &str) -> String {
    format!("{SITE}{path}")
}

/// Render `llms.txt`: the index, with no page bodies.
pub fn render(documents: &[Document]) -> String {
    let mut out = String::new();
    out.push_str("# ethang.dev\n\n");
    out.push_str("> ");
    out.push_str(SUMMARY);
    out.push_str("\n\n");
    out.push_str(GUIDANCE);
    out.push_str("\n\n");
    for (section, documents) in group(documents) {
        out.push_str(&format!("## {}\n\n", section.heading()));
        for document in documents {
            out.push_str(&format!(
                "- [{}]({}): {}\n",
                document.title,
                absolute(&document.path),
                document.note
            ));
        }
        out.push('\n');
    }
    // The spec's `Optional` section is for what a reader can skip. The feed and
    // the sitemap are the two machine-readable files worth naming and neither is
    // a page, so they land here rather than in a list of their own.
    out.push_str("## Optional\n\n");
    out.push_str(&format!(
        "- [RSS feed]({}): every post, newest first, as RSS 2.0.\n",
        escape(&canonical_url(crate::rss::FEED_PATH))
    ));
    out.push_str(&format!(
        "- [Sitemap]({}): every indexable URL.\n",
        canonical_url("/sitemap.xml")
    ));
    out
}

/// Render `llms-full.txt`: the index with every page body inlined.
///
/// The heading structure is one `##` per page under a `##` per section, so a
/// reader that only wants the writing can stop when the next `##` is a section
/// heading it does not want.
pub fn render_full(documents: &[Document]) -> String {
    let mut out = String::new();
    out.push_str("# ethang.dev\n\n");
    out.push_str("> ");
    out.push_str(SUMMARY);
    out.push_str("\n\n");
    out.push_str(GUIDANCE);
    out.push_str("\n\n");
    for (section, documents) in group(documents) {
        out.push_str(&format!("# {}\n\n", section.heading()));
        for document in documents {
            out.push_str(&format!("## {}\n\n", document.title));
            out.push_str(&format!("Source: {}\n\n", absolute(&document.path)));
            out.push_str(document.body.trim());
            out.push_str("\n\n");
        }
    }
    out
}

/// Group the documents by section, each group in the section's own order and
/// each section's documents in the order they were given.
fn group(documents: &[Document]) -> Vec<(Section, Vec<&Document>)> {
    let mut sections: Vec<Section> = documents.iter().map(|document| document.section).collect();
    sections.sort_by_key(|section| section.order());
    sections.dedup();
    sections
        .into_iter()
        .map(|section| {
            let group = documents
                .iter()
                .filter(|document| document.section == section)
                .collect();
            (section, group)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(section: Section, title: &str, path: &str) -> Document {
        Document {
            section,
            title: title.into(),
            path: path.into(),
            note: "What it is.".into(),
            body: format!("# {title}\n\nBody of {title}.\n"),
        }
    }

    #[test]
    fn the_index_opens_with_the_spec_structure() {
        let text = render(&[]);
        assert!(text.starts_with("# ethang.dev\n\n> "), "{text}");
    }

    #[test]
    fn the_index_links_every_document() {
        let documents = [
            document(Section::Pages, "Home", "/index.md"),
            document(Section::Writing, "Alpha", "/blog/alpha/index.md"),
        ];
        let text = render(&documents);
        assert!(text.contains("- [Home](https://ethang.dev/index.md): What it is."));
        assert!(text.contains("- [Alpha](https://ethang.dev/blog/alpha/index.md): What it is."));
    }

    #[test]
    fn sections_are_headed_and_ordered() {
        let documents = [
            document(Section::Tips, "Tip", "/tips/t/index.md"),
            document(Section::Pages, "Home", "/index.md"),
            document(Section::Writing, "Alpha", "/blog/alpha/index.md"),
        ];
        let text = render(&documents);
        let pages = text.find("## Pages").unwrap();
        let writing = text.find("## Writing").unwrap();
        let tips = text.find("## Tips").unwrap();
        assert!(pages < writing && writing < tips, "{text}");
    }

    #[test]
    fn an_empty_section_is_not_headed() {
        let text = render(&[document(Section::Writing, "Alpha", "/blog/alpha/index.md")]);
        assert!(!text.contains("## Pages"));
        assert!(text.contains("## Writing"));
    }

    #[test]
    fn the_index_names_the_feed_and_the_sitemap_as_optional() {
        let text = render(&[]);
        let optional = text.find("## Optional").unwrap();
        assert!(text[optional..].contains("https://ethang.dev/blog/feed.xml"));
        assert!(text[optional..].contains("https://ethang.dev/sitemap.xml"));
    }

    #[test]
    fn the_full_file_inlines_every_body() {
        let documents = [document(Section::Writing, "Alpha", "/blog/alpha/index.md")];
        let text = render_full(&documents);
        assert!(text.contains("## Alpha"));
        assert!(text.contains("Source: https://ethang.dev/blog/alpha/index.md"));
        assert!(text.contains("Body of Alpha."));
    }

    #[test]
    fn the_full_file_names_no_link_it_does_not_inline() {
        // The index's bullet lists are absent from the full file: everything
        // they point at is already here.
        let documents = [document(Section::Writing, "Alpha", "/blog/alpha/index.md")];
        let text = render_full(&documents);
        assert!(!text.contains("- [Alpha]"));
    }
}
