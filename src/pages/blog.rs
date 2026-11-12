//! The blog: the paginated index and one page per post.
//!
//! Both halves are ported from the old Astro site - `blog/index.astro` with
//! `BlogList.astro` and `Pagination.astro` for the list, `blog/[slug].astro`
//! for the article - minus the Tailwind utilities, which became the
//! `.blog-*` classes in `static/styles/site.css`.
//!
//! A page module returns the `<main>` content only: the site shell around it
//! belongs to [`crate::pages::render_doc`].

use crate::content::{BLOG_PAGE_SIZE, Post, max_pages, page_href};
use crate::html::{el, escape};
use crate::pages::{ArticleMeta, Doc, canonical_url};

/// The one-page slice of `posts` that page `page` shows.
///
/// The slice is taken here rather than by the caller so the index and the
/// pagination always agree about which posts belong together: an out-of-range
/// page renders an empty list, exactly as the old `slice` did.
fn page_slice(posts: &[Post], page: usize) -> &[Post] {
    let start = (page.max(1) - 1) * BLOG_PAGE_SIZE;
    let end = start.saturating_add(BLOG_PAGE_SIZE).min(posts.len());
    if start >= end {
        &[]
    } else {
        &posts[start..end]
    }
}

/// Render the blog index body for page `page` (1-based).
///
/// `posts` is the whole blog, newest first; the pagination is derived from its
/// length, so a caller never passes a page count that disagrees with the list.
pub fn render_index(posts: &[Post], page: usize) -> String {
    let page = page.max(1);
    let max_pages = max_pages(posts.len());
    let cards = list_cards(page_slice(posts, page));
    let pagination = pagination(page, max_pages);

    let header = el(
        "div",
        &[("class", "blog-head")],
        &[
            el(
                "p",
                &[("class", "section-kicker")],
                &[escape("writing / notes / field reports")],
            ),
            el("h1", &[("class", "blog-head__title")], &[escape("Blog")]),
            el("div", &[("class", "section-rule")], &[]),
        ],
    );

    el(
        "div",
        &[("class", "page blog")],
        &[header, cards, pagination],
    )
}

/// The list itself: one card per post, or the empty-state card.
fn list_cards(posts: &[Post]) -> String {
    if posts.is_empty() {
        let message = el(
            "span",
            &[("class", "blog-empty__text")],
            &[escape("No blog posts found.")],
        );
        return el("div", &[("class", "card blog-empty")], &[message]);
    }
    let cards: Vec<String> = posts.iter().map(list_card).collect();
    el("div", &[("class", "blog-list")], &cards)
}

/// One post in the list: category kicker, title link, then the `Updated / <date>`
/// line in mono type. `updated_date` wins when present, else `pub_date`.
fn list_card(post: &Post) -> String {
    let mut children = Vec::new();
    if let Some(category) = &post.category {
        children.push(el(
            "p",
            &[("class", "section-kicker blog-card__kicker")],
            &[escape(category)],
        ));
    }
    let href = format!("/blog/{}", post.slug);
    let title = el(
        "a",
        &[("class", "blog-card__link"), ("href", href.as_str())],
        &[escape(&post.title)],
    );
    children.push(el("h2", &[("class", "blog-card__title")], &[title]));
    let stamp = post.updated_date.unwrap_or(post.pub_date);
    children.push(el(
        "p",
        &[("class", "blog-card__stamp")],
        &[
            el(
                "span",
                &[("class", "blog-card__stamp-label")],
                &[escape("Updated")],
            ),
            escape(" / "),
            escape(&format_date(stamp)),
        ],
    ));
    el(
        "article",
        &[("class", "surface-card blog-card"), ("data-blog-post", "")],
        &children,
    )
}

/// The pagination nav: `‹` / numbered pages / `›`, each end a disabled `<span>`
/// when there is nowhere to go. Absent entirely when the blog fits on one page.
fn pagination(page: usize, max_pages: usize) -> String {
    if max_pages <= 1 {
        return String::new();
    }
    let mut items = Vec::with_capacity(max_pages + 2);
    items.push(edge(page, max_pages, false));
    for number in 1..=max_pages {
        items.push(page_number(number, page));
    }
    items.push(edge(page, max_pages, true));
    el(
        "nav",
        &[
            ("class", "blog-pagination"),
            ("aria-label", "Blog pagination"),
        ],
        &items,
    )
}

/// A numbered page link, marked `aria-current="page"` when it is the current one.
fn page_number(number: usize, current: usize) -> String {
    let label = number.to_string();
    let href = page_href(number);
    if number == current {
        return el(
            "a",
            &[
                (
                    "class",
                    "blog-pagination__number blog-pagination__number--current",
                ),
                ("href", href.as_str()),
                ("aria-current", "page"),
            ],
            &[escape(&label)],
        );
    }
    el(
        "a",
        &[
            ("class", "blog-pagination__number"),
            ("href", href.as_str()),
        ],
        &[escape(&label)],
    )
}

/// One end of the pagination: a link when the neighbouring page exists, a
/// disabled `<span>` when it does not. `next` selects which end this is.
fn edge(page: usize, max_pages: usize, next: bool) -> String {
    let (glyph, label, rel) = if next {
        ("\u{203a}", "Next page", "next")
    } else {
        ("\u{2039}", "Previous page", "prev")
    };
    let target = if next {
        (page < max_pages).then(|| page + 1)
    } else {
        (page > 1).then(|| page - 1)
    };
    match target {
        Some(number) => {
            let href = page_href(number);
            el(
                "a",
                &[
                    ("class", "blog-pagination__edge"),
                    ("href", href.as_str()),
                    ("rel", rel),
                    ("aria-label", label),
                ],
                &[escape(glyph)],
            )
        }
        None => el(
            "span",
            &[
                (
                    "class",
                    "blog-pagination__edge blog-pagination__edge--disabled",
                ),
                ("aria-hidden", "true"),
            ],
            &[escape(glyph)],
        ),
    }
}

/// Render a post page body: the title, then the article prose.
pub fn render_post(post: &Post, body_html: &str) -> String {
    let heading = el(
        "h1",
        &[("class", "blog-post__title")],
        &[escape(&post.title)],
    );
    let article = el(
        "article",
        &[("class", "blog-prose")],
        &[body_html.to_string()],
    );
    el("div", &[("class", "page blog-post")], &[heading, article])
}

/// The complete document for one post, shell and all.
///
/// `modified_time` is emitted only when the post was actually revised, matching
/// the old layout's nil check; `published_time` is always the publication date.
pub fn post_doc(post: &Post, body_html: &str) -> Doc {
    Doc {
        title: post.title.clone(),
        description: Some(post_description(post)),
        canonical: Some(canonical_url(&format!("/blog/{}", post.slug))),
        article: Some(ArticleMeta {
            published_time: format_rfc3339(post.pub_date),
            modified_time: post.updated_date.map(format_rfc3339),
        }),
        preload_image: first_image_src(body_html),
        body: render_post(post, body_html),
    }
}

/// The `src` of the first `<img>` in a rendered body, if there is one.
///
/// This is the element most likely to be the page's Largest Contentful Paint,
/// and naming it in the head lets the browser start the request before the
/// parser reaches the tag.
fn first_image_src(html: &str) -> Option<String> {
    let start = html.find("<img")?;
    let tag = &html[start..start + html[start..].find('>')?];
    let needle = "src=\"";
    let value_start = tag.find(needle)? + needle.len();
    let value_end = tag[value_start..].find('"')? + value_start;
    Some(tag[value_start..value_end].to_string())
}

/// The description a post advertises: its frontmatter `description` when it
/// has one, otherwise an excerpt of the body - the fallback the old
/// `[slug].astro` got from `excerptFromMarkdown`.
pub fn post_description(post: &Post) -> String {
    match &post.description {
        Some(description) if !description.is_empty() => description.clone(),
        _ => excerpt(&post.body),
    }
}

/// Longest excerpt the old site produced before it started truncating.
const EXCERPT_LENGTH: usize = 155;

/// Strip the markdown scaffolding from `body` and cut a readable excerpt.
///
/// This is the prose a search result or a link preview shows, so it has to read
/// as a sentence rather than as source: fenced code, images, link targets,
/// heading and list markers, blockquote arrows, and inline emphasis all go.
/// Truncation lands on a word boundary and marks itself with an ellipsis.
fn excerpt(body: &str) -> String {
    let mut text = String::with_capacity(body.len());
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trimmed.starts_with("import ") {
            continue;
        }
        let stripped = strip_inline(line);
        let stripped = stripped.trim();
        if stripped.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(stripped);
    }

    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= EXCERPT_LENGTH {
        return collapsed;
    }
    let head: String = collapsed.chars().take(EXCERPT_LENGTH).collect();
    let cut = head.rfind(' ').map(|index| &head[..index]).unwrap_or(&head);
    format!("{}\u{2026}", cut.trim_end())
}

/// Remove the line-level markdown punctuation a paragraph or list item carries.
fn strip_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line.trim_start();

    // Leading heading hashes, blockquote arrows, and list bullets.
    let bytes = rest.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index] == b'#' {
        index += 1;
    }
    if index > 0 {
        rest = rest[index..].trim_start();
    }
    while let Some(tail) = rest.strip_prefix('>') {
        rest = tail.trim_start();
    }
    for bullet in ["- ", "* ", "+ "] {
        if let Some(tail) = rest.strip_prefix(bullet) {
            rest = tail;
            break;
        }
    }

    let mut chars = rest.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // Emphasis and inline-code markers carry no meaning in plain text.
            '`' | '*' | '_' | '~' => {}
            '[' => {
                // `[label](target)` keeps the label and drops the target.
                out.push_str(&take_label(&mut chars));
                skip_link_target(&mut chars);
            }
            // `![alt](src)` is an image: it contributes nothing to prose.
            '!' if chars.peek() == Some(&'[') => {
                chars.next();
                take_label(&mut chars);
                skip_link_target(&mut chars);
            }
            // A raw HTML tag is scaffolding, not text.
            '<' => {
                for next in chars.by_ref() {
                    if next == '>' {
                        break;
                    }
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Read a `[label` up to its closing `]` and return the label text.
///
/// An unterminated label is taken to the end of the line: the excerpt is prose,
/// so a stray bracket must not swallow everything after it.
fn take_label(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut label = String::new();
    for next in chars.by_ref() {
        if next == ']' {
            break;
        }
        label.push(next);
    }
    label
}

/// Skip a `(target)` that follows a label, counting nested parentheses so a
/// target containing them still ends where it should. A no-op when the next
/// character is not `(`.
fn skip_link_target(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if chars.peek() != Some(&'(') {
        return;
    }
    chars.next();
    let mut depth = 0usize;
    for next in chars.by_ref() {
        match next {
            '(' => depth += 1,
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            _ => {}
        }
    }
}

/// What the index and its pagination advertise. Every page needs a
/// description: Lighthouse's SEO category fails a document without one.
pub const INDEX_DESCRIPTION: &str = "Notes on software engineering, domain-driven design, and the systems behind them.";

/// The blog index as markdown, for its `.md` twin.
///
/// The whole blog rather than one page of it: a markdown file is fetched whole,
/// and pagination is a property of the HTML index that a reader following
/// `llms.txt` should not have to walk.
pub fn render_index_markdown(posts: &[Post]) -> String {
    let mut out = format!("# Blog\n\n{INDEX_DESCRIPTION}\n\n");
    for post in posts {
        let stamp = post.updated_date.unwrap_or(post.pub_date);
        out.push_str(&format!(
            "- [{}]({}/blog/{}/index.md) ({}): {}\n",
            post.title,
            crate::pages::SITE,
            post.slug,
            format_date(stamp),
            post_description(post)
        ));
    }
    out
}

/// The complete document for index page `page`.
pub fn index_doc(posts: &[Post], page: usize) -> Doc {
    let page = page.max(1);
    Doc {
        title: if page == 1 {
            "Blog".to_string()
        } else {
            format!("Blog - Page {page}")
        },
        description: Some(if page == 1 {
            INDEX_DESCRIPTION.to_string()
        } else {
            format!("{INDEX_DESCRIPTION} Page {page}.")
        }),
        canonical: Some(canonical_url(&page_href(page))),
        article: None,
        preload_image: None,
        body: render_index(posts, page),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Split epoch milliseconds into a Gregorian date and a UTC wall clock.
///
/// Howard Hinnant's `civil_from_days` inverts the day count
/// [`parse_date`](crate::content::parse_date) produces, so the two directions
/// stay exact over the whole range a timestamp can occupy.
/// Returns `(year, month, day, hour, minute, second)`.
pub fn civil_from_millis(millis: i64) -> (i64, i64, i64, i64, i64, i64) {
    let seconds = millis.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let second_of_day = seconds.rem_euclid(86_400);

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);

    (
        year,
        month,
        day,
        second_of_day / 3600,
        second_of_day % 3600 / 60,
        second_of_day % 60,
    )
}

/// Format a timestamp the way the old `DateTime.format` call did:
/// `dateStyle: "medium"` plus `timeStyle: "short"`, in UTC.
///
/// e.g. `Sep 5, 2026, 12:00 PM`. The clock is 12-hour with no leading zero on
/// the hour, and midnight reads `12:00 AM`.
pub fn format_date(millis: i64) -> String {
    let (year, month, day, hour, minute, _) = civil_from_millis(millis);
    let month = MONTHS[(month - 1) as usize];
    let hour12 = if hour % 12 == 0 { 12 } else { hour % 12 };
    let meridiem = if hour < 12 { "AM" } else { "PM" };
    format!("{month} {day}, {year}, {hour12}:{minute:02} {meridiem}")
}

/// Format a timestamp as an RFC3339 instant in UTC with milliseconds - the
/// shape `Date.toISOString()` gave `article:published_time`.
pub fn format_rfc3339(millis: i64) -> String {
    let (year, month, day, hour, minute, second) = civil_from_millis(millis);
    let fraction = millis.rem_euclid(1000);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{fraction:03}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(slug: &str, title: &str, pub_date: i64) -> Post {
        Post {
            slug: slug.into(),
            title: title.into(),
            description: None,
            category: None,
            pub_date,
            updated_date: None,
            featured_image: None,
            featured_image_alt: None,
            body: String::new(),
        }
    }

    #[test]
    fn index_lists_each_title_linking_to_its_post() {
        let posts = vec![post("alpha", "Alpha", 0), post("beta", "Beta", 0)];
        let html = render_index(&posts, 1);
        assert!(html.contains("href=\"/blog/alpha\""));
        assert!(html.contains("href=\"/blog/beta\""));
    }

    #[test]
    fn index_shows_the_category_kicker_when_present() {
        let mut p = post("alpha", "Alpha", 0);
        p.category = Some("Engineering".into());
        assert!(render_index(&[p], 1).contains("Engineering"));
    }

    #[test]
    fn pagination_is_absent_on_a_single_page() {
        let html = render_index(&[post("a", "A", 0)], 1);
        assert!(!html.contains("aria-label=\"Blog pagination\""));
    }

    #[test]
    fn pagination_marks_the_current_page() {
        let posts: Vec<Post> = (0..11).map(|i| post(&format!("p{i}"), "T", 0)).collect();
        let html = render_index(&posts, 1);
        assert!(html.contains("aria-label=\"Blog pagination\""));
        assert!(html.contains("aria-current=\"page\""));
        assert!(html.contains("href=\"/blog/page/2\""));
    }

    #[test]
    fn post_page_wraps_the_body_in_blog_prose() {
        let html = render_post(&post("alpha", "Alpha", 0), "<p>Body.</p>");
        assert!(html.contains("class=\"blog-prose\""));
        assert!(html.contains("<p>Body.</p>"));
    }

    #[test]
    fn post_page_escapes_the_title() {
        let html = render_post(&post("alpha", "A & B", 0), "");
        assert!(html.contains("A &amp; B"));
    }

    #[test]
    fn page_one_lists_the_first_page_of_posts_only() {
        let posts: Vec<Post> = (0..11).map(|i| post(&format!("p{i}"), "T", 0)).collect();
        let first = render_index(&posts, 1);
        assert!(first.contains("href=\"/blog/p0\""));
        assert!(first.contains("href=\"/blog/p9\""));
        assert!(!first.contains("href=\"/blog/p10\""));
        let second = render_index(&posts, 2);
        assert!(second.contains("href=\"/blog/p10\""));
        assert!(!second.contains("href=\"/blog/p0\""));
    }

    #[test]
    fn empty_blog_renders_the_empty_state() {
        let html = render_index(&[], 1);
        assert!(html.contains("No blog posts found."));
        assert!(!html.contains("aria-label=\"Blog pagination\""));
    }

    #[test]
    fn pagination_disables_the_edges_it_cannot_follow() {
        let posts: Vec<Post> = (0..21).map(|i| post(&format!("p{i}"), "T", 0)).collect();
        // Page 1 can only go forward, and the dead end is a labelled span.
        let first = render_index(&posts, 1);
        assert!(first.contains("aria-label=\"Next page\""));
        assert!(!first.contains("aria-label=\"Previous page\""));
        assert_eq!(first.matches("blog-pagination__edge--disabled").count(), 1);
        // The last page is the mirror image.
        let last = render_index(&posts, 3);
        assert!(last.contains("aria-label=\"Previous page\""));
        assert!(!last.contains("aria-label=\"Next page\""));
        assert_eq!(last.matches("blog-pagination__edge--disabled").count(), 1);
        assert!(last.contains("href=\"/blog/page/2\""));
        // A middle page can follow both ends.
        let middle = render_index(&posts, 2);
        assert_eq!(middle.matches("blog-pagination__edge--disabled").count(), 0);
        assert!(middle.contains("href=\"/blog\""));
        assert!(middle.contains("href=\"/blog/page/3\""));
    }

    #[test]
    fn the_stamp_prefers_the_updated_date() {
        let mut p = post("alpha", "Alpha", 0);
        p.updated_date = Some(1788609600000);
        assert!(render_index(&[p], 1).contains("Sep 5, 2026, 12:00 PM"));
    }

    #[test]
    fn the_stamp_falls_back_to_the_publication_date() {
        let html = render_index(&[post("alpha", "Alpha", 1704153600000)], 1);
        assert!(html.contains("Jan 2, 2024, 12:00 AM"));
    }

    #[test]
    fn the_stamp_line_reads_updated_slash_date() {
        let html = render_index(&[post("alpha", "Alpha", 0)], 1);
        assert!(html.contains(">Updated</span> / Jan 1, 1970, 12:00 AM"));
    }

    #[test]
    fn dates_format_like_the_old_site() {
        assert_eq!(format_date(1788609600000), "Sep 5, 2026, 12:00 PM");
        assert_eq!(format_date(1704153600000), "Jan 2, 2024, 12:00 AM");
        assert_eq!(format_date(1704197100000), "Jan 2, 2024, 12:05 PM");
        assert_eq!(format_date(1735689540000), "Dec 31, 2024, 11:59 PM");
    }

    #[test]
    fn article_timestamps_are_rfc3339_utc() {
        assert_eq!(format_rfc3339(1704153600000), "2024-01-02T00:00:00.000Z");
        assert_eq!(format_rfc3339(1704153900250), "2024-01-02T00:05:00.250Z");
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_post_document_carries_its_article_metadata() {
        let mut p = post("alpha", "Alpha", 1704153600000);
        p.updated_date = Some(1788609600000);
        p.description = Some("A post.".into());
        let doc = post_doc(&p, "<p>x</p>");
        assert_eq!(doc.title, "Alpha");
        assert_eq!(
            doc.canonical.as_deref(),
            Some("https://ethang.dev/blog/alpha/")
        );
        let article = doc.article.expect("a post is an article");
        assert_eq!(article.published_time, "2024-01-02T00:00:00.000Z");
        assert_eq!(
            article.modified_time.as_deref(),
            Some("2026-09-05T12:00:00.000Z")
        );
    }

    #[test]
    fn a_post_description_falls_back_to_a_body_excerpt() {
        let mut p = post("alpha", "Alpha", 0);
        p.body = "# Heading\n\nThe first real sentence.\n\n```rust\nlet hidden = 1;\n```\n\nSee [the docs](https://example.com) and `code`.\n"
            .into();
        assert_eq!(
            post_description(&p),
            "Heading The first real sentence. See the docs and code."
        );
    }

    #[test]
    fn a_post_description_prefers_the_frontmatter() {
        let mut p = post("alpha", "Alpha", 0);
        p.description = Some("From the frontmatter.".into());
        p.body = "Body prose that must not win.".into();
        assert_eq!(post_description(&p), "From the frontmatter.");
    }

    #[test]
    fn a_long_excerpt_is_cut_on_a_word_boundary() {
        let mut p = post("alpha", "Alpha", 0);
        p.body = "word ".repeat(100);
        let text = post_description(&p);
        assert!(text.ends_with('\u{2026}'));
        assert!(text.chars().count() <= EXCERPT_LENGTH + 1);
        assert!(!text.trim_end_matches('\u{2026}').ends_with(' '));
    }

    #[test]
    fn index_documents_number_their_pages() {
        let posts: Vec<Post> = (0..11).map(|i| post(&format!("p{i}"), "T", 0)).collect();
        let first = index_doc(&posts, 1);
        assert_eq!(first.title, "Blog");
        assert_eq!(first.canonical.as_deref(), Some("https://ethang.dev/blog/"));
        assert!(first.article.is_none());
        let second = index_doc(&posts, 2);
        assert_eq!(second.title, "Blog - Page 2");
        assert_eq!(
            second.canonical.as_deref(),
            Some("https://ethang.dev/blog/page/2/")
        );
    }
}
