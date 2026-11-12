//! `sitemap.xml` and `robots.txt`, built from the routes the site published.
//!
//! The old site got its sitemap from `@astrojs/sitemap`, which walked the
//! built route table. This does the same walk over the list `build_site`
//! collects as it writes, so the sitemap cannot drift from the pages: a route
//! that is not written is not listed, and a route that is written is listed
//! exactly once.
//!
//! The 404 is deliberately absent - it is not a destination.

use crate::html::{el, escape};
use crate::pages::canonical_url;

/// Render a sitemap for `paths`.
///
/// Each entry is a site-absolute path (`/blog/alpha`) and the epoch
/// milliseconds of its last modification, if it has one. The path becomes an
/// absolute URL against [`SITE`] and the stamp becomes `YYYY-MM-DD`, the only
/// precision the sitemap protocol asks for.
pub fn render(paths: &[(String, Option<i64>)]) -> String {
    let urls: Vec<String> = paths
        .iter()
        .map(|(path, lastmod)| {
            // The sitemap names the address the page is served at, which is
            // the trailing-slash form the deployment redirects to.
            let mut children = vec![text_el("loc", &canonical_url(path))];
            if let Some(millis) = lastmod {
                children.push(text_el("lastmod", &format_date(*millis)));
            }
            el("url", &[], &children)
        })
        .collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{}\n</urlset>\n",
        urls.join("\n")
    )
}

/// A `<loc>` or `<lastmod>`: escaped text content, no attributes.
fn text_el(tag: &str, text: &str) -> String {
    el(tag, &[], std::slice::from_ref(&escape(text)))
}

/// The `YYYY-MM-DD` date a `lastmod` carries.
///
/// This is the date half of the RFC3339 instant the article metadata already
/// advertises, cut to the precision the sitemap protocol uses.
pub fn format_date(millis: i64) -> String {
    let stamp = crate::pages::blog::format_rfc3339(millis);
    let date = stamp
        .split_once('T')
        .map_or(stamp.as_str(), |(date, _)| date);
    date.to_string()
}

/// `robots.txt`: the whole site is public, and the sitemap is its index.
pub const ROBOTS: &str = "User-agent: *\nAllow: /\nSitemap: https://ethang.dev/sitemap.xml\n";
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_url_with_absolute_hosts() {
        let xml = render(&[("/".into(), None), ("/blog".into(), Some(1704153600000))]);
        assert!(xml.contains("<loc>https://ethang.dev/</loc>"));
        assert!(xml.contains("<loc>https://ethang.dev/blog/</loc>"));
        assert!(xml.contains("<lastmod>2024-01-02</lastmod>"));
    }

    #[test]
    fn is_a_well_formed_urlset() {
        let xml = render(&[("/".into(), None)]);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));
        assert!(xml.trim_end().ends_with("</urlset>"));
        assert_eq!(xml.matches("<url>").count(), 1);
    }

    #[test]
    fn omits_lastmod_when_a_route_has_no_date() {
        let xml = render(&[("/tips".into(), None)]);
        assert!(!xml.contains("<lastmod>"));
    }

    #[test]
    fn lastmod_is_the_date_of_the_instant() {
        // The article metadata's RFC3339 stamp, cut to its date.
        assert_eq!(format_date(1704153600000), "2024-01-02");
        assert_eq!(format_date(1788609600000), "2026-09-05");
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn an_empty_sitemap_is_still_a_urlset() {
        let xml = render(&[]);
        assert!(xml.contains("<urlset"));
        assert!(!xml.contains("<url>"));
    }

    #[test]
    fn robots_txt_allows_everything_and_names_the_sitemap() {
        assert_eq!(
            ROBOTS,
            "User-agent: *\nAllow: /\nSitemap: https://ethang.dev/sitemap.xml\n"
        );
    }
}
