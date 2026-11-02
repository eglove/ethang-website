//! Page composition and the site build entry point.

use crate::html::{el, escape, raw, void_el};
use std::path::Path;

pub mod blog;
pub mod home;

/// The article metadata a blog post adds to `<head>`.
///
/// Both stamps are RFC3339 with milliseconds and a `Z` designator, which is
/// what the old `BaseLayout` produced by handing `Date.toISOString()` to
/// `article:published_time` / `article:modified_time`.
pub struct ArticleMeta {
    pub published_time: String,
    pub modified_time: Option<String>,
}

/// A complete page: what goes in `<head>` and what goes in `<main>`.
pub struct Doc {
    pub title: String,
    pub description: Option<String>,
    pub canonical: Option<String>,
    /// Present on a blog post; `og:type` becomes `article` when it is.
    pub article: Option<ArticleMeta>,
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
    // `og:type` is always emitted, as the old layout did; a post flips it from
    // `website` to `article` and gains the two article timestamps.
    let og_type = if doc.article.is_some() {
        "article"
    } else {
        "website"
    };
    head.push(void_el(
        "meta",
        &[("property", "og:type"), ("content", og_type)],
    ));
    if let Some(article) = &doc.article {
        head.push(void_el(
            "meta",
            &[
                ("property", "article:published_time"),
                ("content", article.published_time.as_str()),
            ],
        ));
        if let Some(modified) = &article.modified_time {
            head.push(void_el(
                "meta",
                &[
                    ("property", "article:modified_time"),
                    ("content", modified.as_str()),
                ],
            ));
        }
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

/// The content root: `content/` beside the crate.
pub fn content_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content")
}

/// Render the whole site into `out`.
///
/// Each page module contributes its own output; this is the entry point that
/// creates the output tree and (from Task 12) copies `static/` alongside it.
pub fn build_site(out: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out)?;
    let root = content_root();
    let home_data = home::load_home(&root)?;
    let home_body = home::render(&home_data);
    let doc = Doc {
        title: "Ethan Glover".into(),
        description: Some(
            "Software engineer and builder. I build resilient software, clarify complex domains, and leave systems easier to change."
                .into(),
        ),
        canonical: Some(format!("{SITE}/")),
        article: None,
        body: home_body,
    };
    write_page(&out.join("index.html"), &doc)?;
    write_blog(out, &root)?;
    Ok(())
}

/// Write one rendered document, creating the directories it needs.
fn write_page(path: &Path, doc: &Doc) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, render_doc(doc))?;
    Ok(())
}

/// Write every blog route: the index, each paginated page after it, and one
/// page per post.
///
/// Directory-style paths (`blog/<slug>/index.html`) are what the deploy config
/// resolves for the slash-free URLs the old site served.
fn write_blog(out: &Path, root: &Path) -> anyhow::Result<()> {
    let posts = crate::content::load_posts(&root.join("blog"))?;
    let blog_dir = out.join("blog");
    write_page(&blog_dir.join("index.html"), &blog::index_doc(&posts, 1))?;
    for page in 2..=crate::content::max_pages(posts.len()) {
        let doc = blog::index_doc(&posts, page);
        write_page(
            &blog_dir
                .join("page")
                .join(page.to_string())
                .join("index.html"),
            &doc,
        )?;
    }
    for post in &posts {
        let body = crate::markdown::render_markdown(&post.body, &render_ctx(&post.slug));
        let doc = blog::post_doc(post, &body);
        write_page(&blog_dir.join(&post.slug).join("index.html"), &doc)?;
    }
    Ok(())
}

/// The rendering dependencies a post body needs: the slug its assets resolve
/// against, the highlighter, and the LaTeX renderer.
fn render_ctx(slug: &str) -> crate::markdown::RenderCtx<'_> {
    crate::markdown::RenderCtx {
        post_slug: slug,
        highlight: &crate::highlight::highlight,
        render_math: &crate::math::render_math,
    }
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
            article: None,
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
            article: None,
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
            article: None,
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
            article: None,
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
            article: None,
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
    fn build_site_writes_the_home_page() {
        let out = std::env::temp_dir().join("ethang-home-build-test");
        let _ = std::fs::remove_dir_all(&out);
        build_site(&out).unwrap();
        let html = std::fs::read_to_string(out.join("index.html")).unwrap();
        assert!(html.contains("Ethan Glover"));
        assert!(html.contains("13 client projects / 5 internal builds"));
    }

    #[test]
    fn shell_puts_the_body_inside_main() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            article: None,
            body: "<p>hi</p>".into(),
        };
        assert!(render_doc(&doc).contains("<main><p>hi</p></main>"));
    }

    #[test]
    fn build_site_writes_the_blog_index_and_its_pages() {
        let out = std::env::temp_dir().join("ethang-blog-index-build-test");
        let _ = std::fs::remove_dir_all(&out);
        build_site(&out).unwrap();
        let index = std::fs::read_to_string(out.join("blog/index.html")).unwrap();
        assert!(index.contains("<title>Blog</title>"));
        assert!(index.contains("aria-label=\"Blog pagination\""));
        // 37 published posts at 10 per page is 4 pages: 2, 3, and 4 are paged.
        for page in [2, 3, 4] {
            assert!(
                out.join(format!("blog/page/{page}/index.html")).exists(),
                "missing page {page}"
            );
        }
        assert!(!out.join("blog/page/5/index.html").exists());
    }

    #[test]
    fn build_site_writes_a_page_per_post() {
        let out = std::env::temp_dir().join("ethang-blog-post-build-test");
        let _ = std::fs::remove_dir_all(&out);
        build_site(&out).unwrap();
        let page =
            std::fs::read_to_string(out.join("blog/after-spawn-returns/index.html")).unwrap();
        assert!(page.contains("<meta property=\"og:type\" content=\"article\">"));
        assert!(page.contains("article:published_time"));
        assert!(page.contains("class=\"blog-prose\""));
        // The MDX components render: a post image resolves to its built asset.
        assert!(page.contains("/images/blog/after-spawn-returns/"));
    }

    #[test]
    fn shell_marks_a_plain_page_as_a_website() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            article: None,
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.contains("<meta property=\"og:type\" content=\"website\">"));
        assert!(!html.contains("article:published_time"));
    }

    #[test]
    fn shell_marks_an_article_and_emits_its_timestamps() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            article: Some(ArticleMeta {
                published_time: "2024-01-02T00:00:00.000Z".into(),
                modified_time: Some("2024-03-05T13:07:00.000Z".into()),
            }),
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.contains("<meta property=\"og:type\" content=\"article\">"));
        assert!(html.contains(
            "<meta property=\"article:published_time\" content=\"2024-01-02T00:00:00.000Z\">"
        ));
        assert!(html.contains(
            "<meta property=\"article:modified_time\" content=\"2024-03-05T13:07:00.000Z\">"
        ));
    }

    #[test]
    fn shell_omits_the_modified_time_when_the_post_was_never_updated() {
        let doc = Doc {
            title: "T".into(),
            description: None,
            canonical: None,
            article: Some(ArticleMeta {
                published_time: "2024-01-02T00:00:00.000Z".into(),
                modified_time: None,
            }),
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.contains("article:published_time"));
        assert!(!html.contains("article:modified_time"));
    }
}
