//! Page composition and the site build entry point.

use crate::html::{el, escape, raw, void_el};
use crate::images::{PublishedImages, copy_static, publish_post_images};
use anyhow::Context;
use std::collections::BTreeMap;
use std::path::Path;

pub mod blog;
pub mod home;
pub mod learn;
pub mod sitemap;
pub mod tips;

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
    /// A body image to fetch before the parser reaches it.
    ///
    /// The first image in a post body is usually the largest element on the
    /// page. It sits below the intro, so the preload scanner does not reach it
    /// until the HTML above it has been parsed - and that delay is charged to
    /// Largest Contentful Paint. Naming it here starts the request with the
    /// stylesheet instead.
    pub preload_image: Option<String>,
    pub body: String,
}

pub const SITE: &str = "https://ethang.dev";

/// The canonical URL for a site path, in the form the deployment serves.
///
/// `wrangler.jsonc` sets `html_handling: "auto-trailing-slash"`, so every
/// route is served at `/route/` and a slash-free request is answered with a
/// 301. A canonical link, an `og:url`, or a sitemap entry that names the
/// slash-free form therefore points at a redirect rather than the page - two
/// hops for anything that follows it, and a mismatch between what the site
/// says its address is and what the address actually is.
///
/// The root is already a directory, so it keeps its single slash.
pub fn canonical_url(path: &str) -> String {
    if path == "/" {
        return format!("{SITE}/");
    }
    let trimmed = path.trim_end_matches('/');
    format!("{SITE}{trimmed}/")
}

const NAV: [(&str, &str); 4] = [
    ("/", "Home"),
    ("/blog", "Blog"),
    ("/tips", "Tips"),
    ("/learn", "Learn"),
];

/// The description the home page advertises, and the one a 404 reuses.
const HOME_DESCRIPTION: &str = "Software engineer and builder. I build resilient software, clarify complex domains, and leave systems easier to change.";

/// The copy a 404 offers instead of a page.
const NOT_FOUND_LINE: &str = "There is no page at this address. It may have moved, or it may never have existed. The two doors below are always open.";

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
        // Preload the two faces the first screenful actually uses, so they are
        // in flight before the CSS is even parsed and usually land before first
        // paint. `crossorigin` is required on a font preload even same-origin:
        // without it the browser fetches the file twice and the preload is wasted.
        void_el(
            "link",
            &[
                ("rel", "preload"),
                ("href", "/fonts/inter-400.woff2"),
                ("as", "font"),
                ("type", "font/woff2"),
                ("crossorigin", "anonymous"),
            ],
        ),
        void_el(
            "link",
            &[
                ("rel", "preload"),
                ("href", "/fonts/inter-600.woff2"),
                ("as", "font"),
                ("type", "font/woff2"),
                ("crossorigin", "anonymous"),
            ],
        ),
        void_el(
            "link",
            &[("rel", "stylesheet"), ("href", "/styles/site.css")],
        ),
    ];
    if let Some(image) = &doc.preload_image {
        head.push(void_el(
            "link",
            &[
                ("rel", "preload"),
                ("as", "image"),
                ("href", image.as_str()),
                ("fetchpriority", "high"),
            ],
        ));
    }
    head.push(void_el(
        "link",
        &[
            ("rel", "icon"),
            ("type", "image/svg+xml"),
            ("href", "/favicon.svg"),
        ],
    ));
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

/// The static asset root: `static/` beside the crate.
pub fn static_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("static")
}
/// Render the whole site into `out`.
///
/// The order matters: the content is loaded first so a broken frontmatter
/// block fails before anything is written, the images are processed next
/// because that is the slowest step, then `static/` is copied, and only then
/// are the pages written. Each page is added to `routes` as it lands, and that
/// list becomes the sitemap, so a route cannot be published without being
/// listed or listed without being published.
///
/// `out` is created if it does not exist and written into, never cleaned: a
/// stale file from an older build is left alone rather than deleted, because a
/// build that silently removes files is a worse failure than a stale one.
pub fn build_site(out: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(out)?;
    let root = content_root();

    // Load every input before writing any output, so a content error costs
    // nothing and a half-written site is impossible.
    let home_data = home::load_home(&root)?;
    let stages = learn::load_learn(&root)?;
    let tips = tips::load_tips(&root.join("tips"))?;
    let posts = crate::content::load_posts(&root.join("blog"))?;

    // The slowest step by far: 61 source images become 122 WebP variants.
    let assets = publish_images(out, &root, &posts)?;
    copy_static(&static_root(), out)?;

    let mut routes: Vec<(String, Option<i64>)> = Vec::new();

    let home_doc = Doc {
        title: "Ethan Glover".into(),
        description: Some(HOME_DESCRIPTION.into()),
        canonical: Some(canonical_url("/")),
        article: None,
        preload_image: None,
        body: home::render(&home_data),
    };
    write_page(&out.join("index.html"), &home_doc)?;
    routes.push(("/".into(), None));

    write_blog(out, &posts, &assets, &mut routes)?;

    let learn_doc = learn::learn_doc(&stages);
    write_page(&out.join("learn/index.html"), &learn_doc)?;
    routes.push(("/learn".into(), None));

    write_tips(out, &tips, &mut routes)?;

    write_text(&out.join("404.html"), &render_doc(&not_found_doc()))?;
    // The 404 is served by the host, not linked, so it stays out of `routes`.
    write_text(&out.join("sitemap.xml"), &sitemap::render(&routes))?;
    write_text(&out.join("robots.txt"), sitemap::ROBOTS)?;
    // The host answers these before it looks for a file, so the file is the
    // whole implementation: no page is written for either path.
    write_text(&out.join("_redirects"), REDIRECTS)?;
    Ok(())
}

/// Publish every post's images into `out/images/blog/<slug>/`, returning the
/// lookup that tells the markdown stage which file each source became.
///
/// This is the build's slow step - 61 source images become 122 WebP variants,
/// each one decoded, resampled three times, and encoded - so one thread per
/// post runs them at once. Every post owns its own output directory and
/// reads only its own source directory, so there is nothing to share and no
/// ordering to preserve: the result is byte-identical to the serial loop,
/// only sooner.
fn publish_images(
    out: &Path,
    root: &Path,
    posts: &[crate::content::Post],
) -> anyhow::Result<BTreeMap<String, PublishedImages>> {
    std::thread::scope(
        |scope| -> anyhow::Result<BTreeMap<String, PublishedImages>> {
            let handles: Vec<_> = posts
                .iter()
                .map(|post| {
                    let source = root.join("blog").join(&post.slug).join("images");
                    let target = out.join(crate::images::IMAGE_ROOT).join(&post.slug);
                    let slug = post.slug.clone();
                    scope.spawn(move || {
                        publish_post_images(&source, &target)
                            .map(|published| (slug.clone(), published))
                            .with_context(|| format!("publishing the images of {slug}"))
                    })
                })
                .collect();
            let mut published = BTreeMap::new();
            for handle in handles {
                let (slug, images) = handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("an image worker panicked"))??;
                published.insert(slug, images);
            }
            Ok(published)
        },
    )
}

/// Write the tips index and one page per tip.
fn write_tips(
    out: &Path,
    tips: &[tips::Tip],
    routes: &mut Vec<(String, Option<i64>)>,
) -> anyhow::Result<()> {
    let metas: Vec<tips::TipMeta> = tips.iter().map(tips::Tip::meta).collect();
    let tips_dir = out.join("tips");
    write_page(&tips_dir.join("index.html"), &tips::index_doc(&metas))?;
    routes.push(("/tips".into(), None));
    for tip in tips {
        // A tip has no images of its own, so its assets resolve the plain way:
        // rooted at the slug, nothing renamed.
        let resolve = |path: &str| crate::markdown::default_asset_url(&tip.slug, path);
        let body = crate::markdown::render_markdown(
            &tip.body,
            &render_ctx(&tip.slug, &resolve, &|_| None),
        );
        let doc = tips::tip_doc(tip, &body);
        write_page(&tips_dir.join(&tip.slug).join("index.html"), &doc)?;
        routes.push((tips::tip_href(&tip.slug), None));
    }
    Ok(())
}

/// The 404 page: the site shell with an apology and two ways back in.
///
/// The old site had no custom 404 and let the host answer with its own. This
/// one keeps the reader inside the design and points at the two pages that
/// are worth landing on.
pub fn not_found_doc() -> Doc {
    let links = el(
        "ul",
        &[("class", "not-found__links")],
        &[
            el("li", &[], &[el("a", &[("href", "/")], &[escape("Home")])]),
            el(
                "li",
                &[],
                &[el("a", &[("href", "/blog")], &[escape("Blog")])],
            ),
        ],
    );
    let body = el(
        "div",
        &[("class", "page not-found")],
        &[
            el(
                "p",
                &[("class", "section-kicker")],
                &[escape("404 / nothing here")],
            ),
            el(
                "h1",
                &[("class", "not-found__title")],
                &[escape("Page not found")],
            ),
            el("div", &[("class", "section-rule")], &[]),
            el(
                "p",
                &[("class", "prose-muted not-found__text")],
                &[escape(NOT_FOUND_LINE)],
            ),
            links,
        ],
    );
    Doc {
        title: "Page not found".into(),
        description: Some(NOT_FOUND_LINE.into()),
        // A 404 has no canonical URL: it answers for every wrong address.
        canonical: None,
        article: None,
        preload_image: None,
        body,
    }
}

/// The redirects the old site answered with a status code rather than a page.
///
/// The Courses page became Learn. The old site kept the old path working with
/// a `_redirects` rule that the host answers with a 301, and Cloudflare's
/// static assets support the same file, so the rule is ported verbatim. Both
/// spellings are listed because the old file listed both: a bare `/courses`
/// and a trailing-slash `/courses/` arrive as different paths.
///
/// A redirect is not a destination, so it never reaches `routes`: `/learn` is
/// the URL worth indexing, and the old sitemap skipped `/courses` too.
pub const REDIRECTS: &str = "/courses/    /learn    301\n/courses     /learn    301\n";

/// Write one rendered document, creating the directories it needs.
fn write_page(path: &Path, doc: &Doc) -> anyhow::Result<()> {
    write_text(path, &render_doc(doc))
}

/// Write one file, creating the directories it needs.
fn write_text(path: &Path, text: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)?;
    Ok(())
}

/// Write every blog route: the index, each paginated page after it, and one
/// page per post.
///
/// Directory-style paths (`blog/<slug>/index.html`) are what the deploy config
/// resolves for the slash-free URLs the old site served.
fn write_blog(
    out: &Path,
    posts: &[crate::content::Post],
    assets: &BTreeMap<String, PublishedImages>,
    routes: &mut Vec<(String, Option<i64>)>,
) -> anyhow::Result<()> {
    let blog_dir = out.join("blog");
    write_page(&blog_dir.join("index.html"), &blog::index_doc(posts, 1))?;
    routes.push(("/blog".into(), None));
    for page in 2..=crate::content::max_pages(posts.len()) {
        let doc = blog::index_doc(posts, page);
        write_page(
            &blog_dir
                .join("page")
                .join(page.to_string())
                .join("index.html"),
            &doc,
        )?;
        routes.push((crate::content::page_href(page), None));
    }
    for post in posts {
        let published = assets.get(&post.slug);
        let resolve = |path: &str| post_asset_url(&post.slug, published, path);
        let size = |href: &str| published.and_then(|images| images.size_for(href));
        let body = crate::markdown::render_markdown(
            &post.body,
            &render_ctx(&post.slug, &resolve, &size),
        );
        let doc = blog::post_doc(post, &body);
        write_page(&blog_dir.join(&post.slug).join("index.html"), &doc)?;
        routes.push((
            format!("/blog/{}", post.slug),
            Some(post.updated_date.unwrap_or(post.pub_date)),
        ));
    }
    Ok(())
}

/// The rendering dependencies a body needs: the slug its assets resolve
/// against, the highlighter, the LaTeX renderer, and the asset lookup.
///
/// `asset_url` is passed in rather than built here because it borrows the
/// caller's published-image table, which has to outlive the context.
fn render_ctx<'a>(
    slug: &'a str,
    asset_url: &'a dyn Fn(&str) -> String,
    asset_size: &'a dyn Fn(&str) -> Option<(u32, u32)>,
) -> crate::markdown::RenderCtx<'a> {
    crate::markdown::RenderCtx {
        post_slug: slug,
        highlight: &crate::highlight::highlight,
        render_math: &crate::math::render_math,
        asset_url,
        asset_size,
    }
}

/// Where a post's `<img>` points: the variant the build published for that
/// source file, or - when the build published nothing for it - the plain
/// rooted path, so a broken reference is visible rather than silent.
fn post_asset_url(slug: &str, published: Option<&PublishedImages>, path: &str) -> String {
    match published.and_then(|images| images.href_for(path)) {
        Some(href) => href.to_string(),
        None => crate::markdown::default_asset_url(slug, path),
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
        preload_image: None,
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
        preload_image: None,
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
        preload_image: None,
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
        preload_image: None,
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
        preload_image: None,
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
        preload_image: None,
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
        preload_image: None,
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
            preload_image: None,
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
            preload_image: None,
            body: String::new(),
        };
        let html = render_doc(&doc);
        assert!(html.contains("article:published_time"));
        assert!(!html.contains("article:modified_time"));
    }

    #[test]
    fn sitemap_lists_every_url_with_absolute_hosts() {
        let xml = sitemap::render(&[("/".into(), None), ("/blog".into(), Some(1704153600000))]);
        assert!(xml.contains("<loc>https://ethang.dev/</loc>"));
        assert!(xml.contains("<loc>https://ethang.dev/blog/</loc>"));
        assert!(xml.contains("<lastmod>2024-01-02</lastmod>"));
    }

    #[test]
    fn tips_index_links_each_tip() {
        let html = tips::render_index(&[tips::TipMeta {
            slug: "scrollbar-gutter".into(),
            title: "scrollbar-gutter".into(),
        }]);
        assert!(html.contains("href=\"/tips/scrollbar-gutter\""));
    }

    #[test]
    fn build_site_writes_every_route() {
        let out = site();
        for path in [
            "index.html",
            "blog/index.html",
            "learn/index.html",
            "tips/index.html",
            "404.html",
            "sitemap.xml",
            "robots.txt",
            "styles/site.css",
        ] {
            assert!(out.join(path).exists(), "missing {path}");
        }
    }

    #[test]
    fn build_site_emits_a_page_per_post() {
        let out = site();
        assert!(out.join("blog/sessions-are-agents/index.html").exists());
    }

    #[test]
    fn build_site_emits_a_page_per_tip() {
        let out = site();
        for slug in ["scroll-containers", "scrollbar-gutter"] {
            let page = std::fs::read_to_string(out.join(format!("tips/{slug}/index.html")))
                .unwrap_or_else(|error| panic!("missing tips/{slug}: {error}"));
            assert!(page.contains("class=\"blog-prose tip-prose\""));
        }
        // A tip body is markdown, so its fence is highlighted like a post's:
        // the property is split across spans, so look for its pieces.
        let gutter = read("tips/scrollbar-gutter/index.html");
        assert!(gutter.contains("<pre class=\"code\" tabindex=\"0\"><code>"));
        assert!(gutter.contains("scrollbar-gutter</span>"));
        assert!(gutter.contains("both</span>"));
        // Raw HTML in a tip body passes through as markup, not as text. The
        // fence closes the paragraph above it, so the demo markup is indented
        // by two spaces: pulldown-cmark treats that as a continuation.
        assert!(gutter.contains("tip-demo-panel__scroll--stable"));
    }

    #[test]
    fn the_tips_index_lists_both_tips() {
        let index = read("tips/index.html");
        assert!(index.contains("href=\"/tips/scroll-containers\""));
        assert!(index.contains("href=\"/tips/scrollbar-gutter\""));
        assert!(index.contains("<h1 class=\"tips-head__title\">Tips</h1>"));
    }

    #[test]
    fn build_site_publishes_post_images_and_static_assets() {
        let out = site();
        assert!(out.join("favicon.svg").exists());
        assert!(out.join("fonts/inter-400.woff2").exists());
        assert!(out.join("diagrams/the-road-to-blowup.svg").exists());
        // A post image is republished as a WebP variant, not as its source PNG.
        let images = out.join("images/blog/after-spawn-returns");
        let published: Vec<String> = std::fs::read_dir(&images)
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            published.iter().any(|name| name.ends_with(".webp")),
            "no webp variants: {published:?}"
        );
    }

    #[test]
    fn the_courses_redirect_answers_both_spellings() {
        // The host matches these paths before it looks for a file, so the
        // rule is the whole implementation and both spellings must be here:
        // `/courses` and `/courses/` arrive as different paths.
        let rules: Vec<&str> = REDIRECTS.lines().collect();
        assert_eq!(rules.len(), 2, "expected two rules: {REDIRECTS}");
        for (rule, from) in rules.iter().zip(["/courses/", "/courses"]) {
            let fields: Vec<&str> = rule.split_whitespace().collect();
            assert_eq!(fields, [from, "/learn", "301"], "wrong rule: {rule}");
        }
    }

    #[test]
    fn build_site_writes_the_redirects_file() {
        let out = site();
        let redirects = std::fs::read_to_string(out.join("_redirects")).unwrap();
        assert_eq!(redirects, REDIRECTS);
        // A redirect is not a destination: no page is written for it, and it
        // stays out of the sitemap.
        assert!(!out.join("courses/index.html").exists());
        let sitemap = std::fs::read_to_string(out.join("sitemap.xml")).unwrap();
        assert!(!sitemap.contains("/courses"));
    }

    #[test]
    fn the_404_page_offers_the_way_back() {
        let page = read("404.html");
        assert!(page.contains("Page not found"));
        assert!(page.contains("<a href=\"/\">Home</a>"));
        assert!(page.contains("<a href=\"/blog\">Blog</a>"));
        // It is a page, not an article, and it claims no canonical URL.
        assert!(page.contains("content=\"website\""));
        assert!(!page.contains("rel=\"canonical\""));
    }

    #[test]
    fn the_sitemap_lists_every_page_but_the_404() {
        let xml = read("sitemap.xml");
        for loc in [
            "<loc>https://ethang.dev/</loc>",
            "<loc>https://ethang.dev/blog/</loc>",
            "<loc>https://ethang.dev/blog/page/2/</loc>",
            "<loc>https://ethang.dev/blog/sessions-are-agents/</loc>",
            "<loc>https://ethang.dev/learn/</loc>",
            "<loc>https://ethang.dev/tips/</loc>",
            "<loc>https://ethang.dev/tips/scrollbar-gutter/</loc>",
        ] {
            assert!(xml.contains(loc), "sitemap is missing {loc}");
        }
        assert!(!xml.contains("404"));
    }

    #[test]
    fn blog_posts_carry_their_lastmod_in_the_sitemap() {
        let xml = read("sitemap.xml");
        // One <url> block per post, each with a <lastmod> of its own date.
        let post = xml
            .split("<url>")
            .find(|block| block.contains("https://ethang.dev/blog/sessions-are-agents"))
            .expect("the post is listed");
        assert!(post.contains("<lastmod>"), "no lastmod: {post}");
        let stamp = post
            .split("<lastmod>")
            .nth(1)
            .and_then(|rest| rest.split('<').next())
            .unwrap();
        assert_eq!(stamp.len(), 10, "not a YYYY-MM-DD date: {stamp}");
        assert_eq!(stamp.matches('-').count(), 2, "not a date: {stamp}");
    }

    #[test]
    fn robots_txt_names_the_sitemap() {
        let robots = read("robots.txt");
        assert_eq!(
            robots,
            "User-agent: *\nAllow: /\nSitemap: https://ethang.dev/sitemap.xml\n"
        );
    }

    #[test]
    fn every_internal_link_in_the_built_site_resolves() {
        let out = site();
        let missing = broken_links(out);
        assert!(
            missing.is_empty(),
            "unresolved internal links: {missing:#?}"
        );
    }

    use std::collections::BTreeSet;

    /// The one full build every build test reads from.
    ///
    /// Building is the only way to test what a route actually contains, and
    /// these tests need the real content: the corpus is the fixture. It is
    /// also the expensive step - the image pass decodes and re-encodes 61
    /// photographs - so it happens once, behind a `OnceLock`, and every test
    /// below asserts against the same output tree. A test that cleared the
    /// directory first would race the others; nothing here mutates it.
    fn site() -> &'static Path {
        static OUT: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        OUT.get_or_init(|| {
            let out = std::env::temp_dir().join("ethang-site-build");
            let _ = std::fs::remove_dir_all(&out);
            build_site(&out).unwrap();
            out
        })
    }

    /// Read a file out of the built site, reporting which one was missing.
    fn read(relative: &str) -> String {
        let path = site().join(relative);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()))
    }

    /// Every `href` and `src` in the site's HTML that points inside the site
    /// and does not resolve to a file in the output tree.
    ///
    /// An internal link is one that starts with `/` and is not protocol
    /// relative (`//`). The query and fragment are dropped, then the target is
    /// looked up the way the host will: a path ending in `/` is a directory
    /// with an `index.html`, and anything else is either a file or a directory
    /// with an `index.html` (the deploy config resolves `/blog/x` to
    /// `blog/x/index.html`).
    fn broken_links(out: &Path) -> BTreeSet<String> {
        let mut broken = BTreeSet::new();
        let mut html = Vec::new();
        collect_html(out, &mut html);
        for page in &html {
            let text = std::fs::read_to_string(page).unwrap();
            for target in link_targets(&text) {
                if !target.starts_with('/') || target.starts_with("//") {
                    continue;
                }
                let path = target.split(['?', '#']).next().unwrap_or("");
                if !resolves(out, path) {
                    broken.insert(format!("{} -> {target}", page.display()));
                }
            }
        }
        broken
    }

    /// Every `.html` file under `dir`, recursively.
    fn collect_html(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_html(&path, out);
            } else if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
                out.push(path);
            }
        }
    }

    /// The `href` and `src` attribute values in one HTML document.
    fn link_targets(text: &str) -> Vec<String> {
        let mut targets = Vec::new();
        for attribute in ["href=\"", "src=\""] {
            let mut rest = text;
            while let Some(at) = rest.find(attribute) {
                rest = &rest[at + attribute.len()..];
                if let Some(end) = rest.find('"') {
                    targets.push(rest[..end].to_string());
                    rest = &rest[end..];
                }
            }
        }
        targets
    }

    /// Whether a site-absolute `path` resolves to something in `out`.
    fn resolves(out: &Path, path: &str) -> bool {
        let relative = path.trim_start_matches('/');
        if relative.is_empty() {
            return out.join("index.html").exists();
        }
        let target = out.join(relative);
        if path.ends_with('/') {
            return target.join("index.html").exists();
        }
        target.is_file() || target.join("index.html").exists()
    }
}
