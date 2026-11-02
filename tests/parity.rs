//! Parity: every public route of the old Astro site, and nothing it dropped.
//!
//! The old site is read-only reference, so these tests do not consult it. The
//! route list below is the verified set of published pages, frozen here on
//! purpose: a test that regenerated it from the old repo would agree with any
//! future deletion. The only copy of the truth is this file.
//!
//! Every test reads the committed `dist/` from `cargo run --release -- build
//! --out dist`, not a temporary build, so a stale output tree fails the suite
//! rather than a fresh one quietly passing it.

use std::path::{Path, PathBuf};

fn dist() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("dist")
}

/// The 37 published posts, as the old site served them.
///
/// The old content directory holds 39: `the-harness-is-the-orchestrator` and
/// `three-channels-for-reasoning` carry only an `images/` folder and no
/// `index.mdx`, so they were never published and were not migrated.
const OLD_SLUGS: [&str; 37] = [
    "after-spawn-returns",
    "cognitive-debt-and-the-fragile-state-of-tech",
    "control-consent-and-camera-shake",
    "development-databases-and-the-history-of-domains",
    "development-jobs-trending-upward",
    "documenting-the-invisible-risks-and-hidden-realities-of-current-tech",
    "eslint-upgrades-malicious-free-vpns-and-the-gating-of-gpt-5-6",
    "from-ide-origins-to-junie-cli-and-air",
    "global-tensions-and-tech-industry-disruptions",
    "grid-volatility-resource-shifting-and-content-caches",
    "half-right",
    "i-ve-been-moving-my-projects-to-microservices-behind-a-graphql-gateway-and-everything-has-been",
    "implementation-details-are-a-choice",
    "improving-accessibility-with-eslint-the-require-details-summary-rule",
    "improving-code-safety-new-eslint-rules-for-angular-signals-and-templates",
    "kimi-k3-gemini-3-6-flash-cloudflare-r2-and-ai-intent-metrics",
    "making-secure-behavior-the-easiest-option",
    "model-picker-building-a-model-router-that-actually-respects-your-budget",
    "model-picker-paid-classification-and-the-0-05-per-million-reality",
    "modern-web-standards-and-the-evolution-of-frontend-architecture",
    "one-prompt-many-safeguards-the-architecture-of-a-pi-agent",
    "programming-is-not-a-skill",
    "quantum-standards-web-specs-and-monopoly-bulwarks",
    "sessions-are-agents",
    "skills-that-teach-the-scientific-method",
    "software-foes-sovereign-agents-and-supreme-court-decisions",
    "surface-tension",
    "systems-and-consequences",
    "the-architecture-of-ethang-agent",
    "the-barrier-is-rarely-the-work",
    "the-blowup-half",
    "the-death-of-open-ai-500-point-languages-and-the-human-element",
    "the-real-prodigal",
    "typescript-7-vite-beta-and-the-reality-of-gpt-5",
    "what-sets-ethang-agent-apart",
    "where-the-evaluator-lives",
    "wtf-is-vinext",
];

/// Every `.html` file under `dir`, recursively, in a stable order.
fn pages(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(dir, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("html") {
            out.push(path);
        }
    }
}

#[test]
fn every_old_post_has_a_page() {
    for slug in OLD_SLUGS {
        let page = dist().join("blog").join(slug).join("index.html");
        assert!(page.exists(), "missing post page for {slug}");
    }
}

#[test]
fn every_old_route_has_a_page() {
    // The routes that are neither a post nor a paginated page.
    for route in [
        "index.html",
        "blog/index.html",
        "learn/index.html",
        "tips/index.html",
        "tips/scroll-containers/index.html",
        "tips/scrollbar-gutter/index.html",
        "404.html",
    ] {
        assert!(dist().join(route).exists(), "missing route /{route}");
    }
}

#[test]
fn every_blog_index_page_has_a_page() {
    // 37 posts at 10 per page is 4 pages: /blog plus page/2, page/3, page/4.
    assert!(dist().join("blog/index.html").exists(), "missing /blog");
    for page in 2..=4 {
        let path = dist().join("blog").join("page").join(page.to_string());
        assert!(
            path.join("index.html").exists(),
            "missing /blog/page/{page}"
        );
    }
    assert!(
        !dist().join("blog/page/5").exists(),
        "/blog/page/5 should not exist: 37 posts fit in 4 pages"
    );
}

#[test]
fn dropped_routes_are_absent() {
    // /rss, /applications and /login were server-rendered or private; the
    // user deferred them. /applications/<id>/resume went with /applications.
    for route in ["rss", "applications", "login"] {
        assert!(
            !dist().join(route).exists(),
            "/{route} should have been dropped"
        );
    }
}

#[test]
fn the_draft_posts_were_not_published() {
    for slug in [
        "the-harness-is-the-orchestrator",
        "three-channels-for-reasoning",
    ] {
        assert!(
            !dist().join("blog").join(slug).exists(),
            "/blog/{slug} is an unpublished draft"
        );
    }
}

#[test]
fn the_courses_redirect_landed_on_learn() {
    // The old site redirected /courses to /learn with a rule in `_redirects`,
    // and Cloudflare's static assets answer the same file with a real 301.
    // A static host cannot redirect from an HTML page, so the rule is the
    // port; the page it replaced must not be written.
    let path = dist().join("_redirects");
    let redirects = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    assert!(
        redirects.contains("/courses ") || redirects.contains("/courses/ "),
        "_redirects has no /courses rule: {redirects}"
    );
    for line in redirects
        .lines()
        .filter(|line| line.starts_with("/courses"))
    {
        let fields: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(fields.get(1), Some(&"/learn"), "wrong target: {line}");
        assert_eq!(fields.get(2), Some(&"301"), "wrong status: {line}");
    }
    assert!(
        !dist().join("courses").exists(),
        "/courses is a redirect, not a page: no directory should exist"
    );
    let sitemap = std::fs::read_to_string(dist().join("sitemap.xml")).unwrap();
    assert!(
        !sitemap.contains("/courses"),
        "/courses is a redirect and does not belong in the sitemap"
    );
}
#[test]
fn no_page_is_empty() {
    let pages = pages(&dist());
    // 1 home + 1 blog index + 3 paged (page/2..page/4) + 37 posts + 1 learn
    // + 1 tips index + 2 tips + 1 404 = 47. `/courses` is a `_redirects` rule,
    // not a page, so it is not counted. The bound is a floor rather than an
    // equality so that publishing a post does not have to edit this file; it
    // still fails the moment a route is lost.
    assert!(
        pages.len() >= 47,
        "expected at least 47 pages, found {}",
        pages.len()
    );
    for page in pages {
        let html = std::fs::read_to_string(&page).unwrap();
        assert!(html.contains("<title>"), "{} has no title", page.display());
        assert!(html.len() > 500, "{} looks empty", page.display());
    }
}

#[test]
fn every_internal_link_resolves() {
    let mut broken = Vec::new();
    for page in pages(&dist()) {
        let html = std::fs::read_to_string(&page).unwrap();
        for target in link_targets(&html) {
            if !target.starts_with('/') || target.starts_with("//") {
                continue;
            }
            if !resolves(&target) {
                broken.push(format!("{} -> {target}", page.display()));
            }
        }
    }
    assert!(
        broken.is_empty(),
        "broken internal links:\n{}",
        broken.join("\n")
    );
}

/// The `href` and `src` values in one HTML document.
fn link_targets(html: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for attribute in ["href=\"", "src=\""] {
        let mut rest = html;
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

/// Whether a site-absolute `target` resolves to something in `dist/`.
///
/// `/x` resolves to `dist/x/index.html` (every route is emitted
/// directory-style), to `dist/x.html`, or to `dist/x` itself for an asset.
/// The query and fragment are dropped first: a `#` anchor is the same page.
fn resolves(target: &str) -> bool {
    let path = target.split(['?', '#']).next().unwrap_or("");
    let relative = path.trim_start_matches('/');
    let base = dist();
    if relative.is_empty() {
        return base.join("index.html").exists();
    }
    let direct = base.join(relative);
    let as_page = direct.join("index.html").exists();
    let as_file = base.join(format!("{relative}.html")).exists();
    as_page || as_file || direct.is_file()
}
