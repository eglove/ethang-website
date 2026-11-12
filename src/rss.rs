//! The blog's RSS feed: `/blog/feed.xml`.
//!
//! RSS 2.0, with the Atom namespace declared for the one element RSS lacks: a
//! `<atom:link rel="self">`, so a reader that has only the feed's contents can
//! find the URL that keeps it current. Everything else is core RSS 2.0, which
//! every reader understands.
//!
//! The feed carries the same description the page advertises rather than the
//! whole article. A reader shows the description in its list, and a full-content
//! feed would repeat every post's HTML in a second file that nothing checks.

use crate::content::Post;
use crate::html::escape;
use crate::pages::{blog::civil_from_millis, canonical_url};

/// Where the feed lives, as a site path.
pub const FEED_PATH: &str = "/blog/feed.xml";

/// The feed's title, as a reader lists it.
pub const FEED_TITLE: &str = "ethang.dev";

/// What the feed says it is.
pub const FEED_DESCRIPTION: &str =
    "Notes on software engineering, domain-driven design, and the systems behind them.";

/// Render the feed for `posts`, newest first.
///
/// `posts` is the whole blog: a feed is the one list on the site that is not
/// paginated, because a reader fetches the file whole. `description` is the
/// per-post summary the index already computes, so the two never disagree.
pub fn render(posts: &[Post], description: &dyn Fn(&Post) -> String) -> String {
    let mut items = String::new();
    for post in posts {
        let link = canonical_url(&format!("/blog/{}", post.slug));
        let stamp = post.updated_date.unwrap_or(post.pub_date);
        items.push_str(&format!(
            "    <item>\n      <title>{}</title>\n      <link>{}</link>\n      <guid isPermaLink=\"true\">{}</guid>\n      <pubDate>{}</pubDate>\n      <description>{}</description>\n    </item>\n",
            escape(&post.title),
            escape(&link),
            escape(&link),
            rfc822(stamp),
            escape(&description(post)),
        ));
    }
    // The channel's own timestamp is the newest post's, so the feed does not
    // depend on the clock: a rebuild with no content change is byte-identical.
    let newest = posts.iter().map(|post| post.updated_date.unwrap_or(post.pub_date)).max();
    let last_build = newest.map(rfc822).unwrap_or_default();
    format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">\n",
            "  <channel>\n",
            "    <title>{title}</title>\n",
            "    <link>{link}</link>\n",
            "    <atom:link href=\"{self_link}\" rel=\"self\" type=\"application/rss+xml\" />\n",
            "    <description>{description}</description>\n",
            "    <language>en</language>\n",
            "    <lastBuildDate>{last_build}</lastBuildDate>\n",
            "    <managingEditor>hello@ethang.email (Ethan Glover)</managingEditor>\n",
            "    <copyright>Ethan Glover</copyright>\n",
            "{items}  </channel>\n",
            "</rss>\n",
        ),
        title = FEED_TITLE,
        link = escape(&canonical_url("/blog")),
        self_link = escape(&crate::llms::absolute(FEED_PATH)),
        description = escape(FEED_DESCRIPTION),
        last_build = last_build,
        items = items,
    )
}

const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Format an instant the way RFC 822 wants a `<pubDate>`:
/// `Tue, 02 Jan 2024 00:00:00 +0000`.
///
/// RSS 2.0 pins `<pubDate>` to RFC 822, which predates ISO 8601 and has no
/// timezone database behind it. Every timestamp the site carries is UTC, so the
/// zone is always `+0000` and the conversion is arithmetic.
pub fn rfc822(millis: i64) -> String {
    let seconds = millis.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);
    let (year, month, day, _, _, _) = civil_from_millis(millis);
    // 1970-01-01 was a Thursday, and a week is seven days, so the day of the
    // week is the day count modulo seven.
    let weekday = DAYS[days.rem_euclid(7) as usize];
    let month = MONTHS[(month - 1) as usize];
    format!(
        "{weekday}, {day:02} {month} {year} {:02}:{:02}:{:02} +0000",
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(slug: &str, title: &str, pub_date: i64) -> Post {
        Post {
            slug: slug.into(),
            title: title.into(),
            description: Some("A post.".into()),
            category: None,
            pub_date,
            updated_date: None,
            featured_image: None,
            featured_image_alt: None,
            body: String::new(),
        }
    }

    #[test]
    fn rfc822_names_the_weekday_of_the_instant() {
        // 2024-01-02T00:00:00Z was a Tuesday; 2026-09-05T12:00:00Z a Saturday.
        assert_eq!(rfc822(1_704_153_600_000), "Tue, 02 Jan 2024 00:00:00 +0000");
        assert_eq!(rfc822(1_788_609_600_000), "Sat, 05 Sep 2026 12:00:00 +0000");
        assert_eq!(rfc822(0), "Thu, 01 Jan 1970 00:00:00 +0000");
    }

    #[test]
    fn rfc822_pads_every_field() {
        assert_eq!(rfc822(1_735_689_540_000), "Tue, 31 Dec 2024 23:59:00 +0000");
    }

    #[test]
    fn a_feed_is_a_well_formed_rss_document() {
        let xml = render(&[post("alpha", "Alpha", 1_704_153_600_000)], &|p| {
            p.description.clone().unwrap_or_default()
        });
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(xml.contains("<rss version=\"2.0\" xmlns:atom=\"http://www.w3.org/2005/Atom\">"));
        assert!(xml.contains("<title>ethang.dev</title>"));
        assert!(xml.contains("<link>https://ethang.dev/blog/</link>"));
        assert!(xml.trim_end().ends_with("</rss>"));
    }

    #[test]
    fn the_self_link_points_at_the_feed() {
        let xml = render(&[post("alpha", "Alpha", 0)], &|_| String::new());
        assert!(xml.contains(
            "<atom:link href=\"https://ethang.dev/blog/feed.xml\" rel=\"self\" type=\"application/rss+xml\" />"
        ));
    }

    #[test]
    fn every_post_becomes_an_item_linking_to_its_page() {
        let posts = vec![
            post("alpha", "Alpha", 1_704_153_600_000),
            post("beta", "Beta", 1_704_239_600_000),
        ];
        let xml = render(&posts, &|p| p.description.clone().unwrap_or_default());
        assert_eq!(xml.matches("<item>").count(), 2);
        assert!(xml.contains("<link>https://ethang.dev/blog/alpha/</link>"));
        assert!(xml.contains("<guid isPermaLink=\"true\">https://ethang.dev/blog/alpha/</guid>"));
    }

    #[test]
    fn items_keep_the_order_they_were_given() {
        // The caller sorts newest first; the feed must not resort and disagree.
        let posts = vec![
            post("new", "New", 1_704_239_600_000),
            post("old", "Old", 1_704_153_600_000),
        ];
        let xml = render(&posts, &|_| String::new());
        let new_at = xml.find("/blog/new/").unwrap();
        let old_at = xml.find("/blog/old/").unwrap();
        assert!(new_at < old_at);
    }

    #[test]
    fn a_title_with_markup_is_escaped() {
        let xml = render(&[post("alpha", "A & B <c>", 0)], &|_| String::new());
        assert!(xml.contains("<title>A &amp; B &lt;c&gt;</title>"));
    }

    #[test]
    fn last_build_date_is_the_newest_post() {
        let mut newer = post("beta", "Beta", 1_704_239_600_000);
        newer.updated_date = Some(1_788_609_600_000);
        let xml = render(&[post("alpha", "Alpha", 0), newer], &|_| String::new());
        assert!(xml.contains("<lastBuildDate>Sat, 05 Sep 2026 12:00:00 +0000</lastBuildDate>"));
    }

    #[test]
    fn an_empty_feed_is_still_a_channel() {
        let xml = render(&[], &|_| String::new());
        assert!(xml.contains("<channel>"));
        assert!(!xml.contains("<item>"));
    }
}
