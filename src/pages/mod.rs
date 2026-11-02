//! Page composition and the site build entry point.

use crate::html::{el, escape, raw, void_el};
use std::path::Path;

/// A complete page: what goes in `<head>` and what goes in `<main>`.
pub struct Doc {
    pub title: String,
    pub description: Option<String>,
    pub canonical: Option<String>,
    pub body: String,
}

pub const SITE: &str = "https://ethang.dev";

const NAV: [(&str, &str); 4] = [
    ("/", "Home"),
    ("/blog", "Blog"),
    ("/tips", "Tips"),
    ("/learn", "Learn"),
];

/// Wrap `doc` in the shared `<html>` shell: head metadata, site nav, footer.
pub fn render_doc(doc: &Doc) -> String {
    let mut head = vec![
        void_el("meta", &[("charset", "utf-8")]),
        void_el(
            "meta",
            &[
                ("name", "viewport"),
                ("content", "width=device-width, initial-scale=1"),
            ],
        ),
        void_el("meta", &[("name", "theme-color"), ("content", "#011627")]),
        el("title", &[], &[escape(&doc.title)]),
        void_el(
            "link",
            &[("rel", "stylesheet"), ("href", "/styles/site.css")],
        ),
        void_el(
            "link",
            &[
                ("rel", "icon"),
                ("type", "image/svg+xml"),
                ("href", "/favicon.svg"),
            ],
        ),
    ];
    if let Some(description) = &doc.description {
        head.push(void_el(
            "meta",
            &[("name", "description"), ("content", description.as_str())],
        ));
        head.push(void_el(
            "meta",
            &[
                ("property", "og:description"),
                ("content", description.as_str()),
            ],
        ));
    }
    if let Some(canonical) = &doc.canonical {
        head.push(void_el(
            "link",
            &[("rel", "canonical"), ("href", canonical.as_str())],
        ));
        head.push(void_el(
            "meta",
            &[("property", "og:url"), ("content", canonical.as_str())],
        ));
    }
    let nav_items: Vec<String> = NAV
        .iter()
        .map(|(href, label)| el("li", &[], &[el("a", &[("href", href)], &[escape(label)])]))
        .collect();
    let nav = el(
        "nav",
        &[("class", "site-nav"), ("aria-label", "Main")],
        &[
            el(
                "a",
                &[("href", "/"), ("class", "site-mark")],
                &[raw("<span class=\"site-mark__tilde\">~/</span>ethang.dev")],
            ),
            el("ul", &[], &nav_items),
        ],
    );
    let footer = el(
        "footer",
        &[("class", "site-footer")],
        &[
            raw("<div class=\"section-rule\"></div>"),
            el(
                "p",
                &[],
                &[escape(
                    "Written by Ethan Glover. Built with Rust, shipped as static HTML.",
                )],
            ),
        ],
    );
    let body = el(
        "body",
        &[],
        &[
            el("header", &[("class", "site-header")], &[nav]),
            el("main", &[], std::slice::from_ref(&doc.body)),
            footer,
        ],
    );
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n{}\n</head>\n{}\n</html>\n",
        head.join("\n"),
        body
    )
}

/// Render the whole site into `out`.
///
/// Each page module contributes its own output; this is the entry point that
/// creates the output tree and (from Task 12) copies `static/` alongside it.
pub fn build_site(out: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_has_doctype_lang_and_stylesheet() {
        let doc = Doc {
            title: "Test".into(),
            description: None,
            canonical: None,
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.starts_with("<!doctype html>"));
        assert!(html.contains("<html lang=\"en\">"));
        assert!(html.contains("href=\"/styles/site.css\""));
    }

    #[test]
    fn shell_escapes_the_title() {
        let doc = Doc {
            title: "a & b".into(),
            description: None,
            canonical: None,
            body: String::new(),
        };
        assert!(render_doc(&doc).contains("<title>a &amp; b</title>"));
    }

    #[test]
    fn shell_omits_description_when_absent() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            body: String::new(),
        };
        assert!(!render_doc(&doc).contains("name=\"description\""));
    }

    #[test]
    fn shell_emits_description_and_canonical_when_present() {
        let doc = Doc {
            title: "T".into(),
            description: Some("a & b".into()),
            canonical: Some("https://ethang.dev/x".into()),
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.contains("<meta name=\"description\" content=\"a &amp; b\">"));
        assert!(html.contains("<link rel=\"canonical\" href=\"https://ethang.dev/x\">"));
    }

    #[test]
    fn shell_renders_nav_and_footer() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            body: String::new(),
        };
        let html = render_doc(&doc);
        for href in ["/blog", "/tips", "/learn"] {
            assert!(
                html.contains(&format!("href=\"{href}\"")),
                "missing nav link {href}"
            );
        }
        assert!(html.contains("<footer class=\"site-footer\">"));
        assert!(html.contains("<main>"));
    }

    #[test]
    fn shell_puts_the_body_inside_main() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            body: "<p>hi</p>".into(),
        };
        assert!(render_doc(&doc).contains("<main><p>hi</p></main>"));
    }
}
