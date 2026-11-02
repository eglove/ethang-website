//! Frontmatter parsing plus post loading, sorting, and pagination.
//!
//! Posts live at `<root>/<directory>/index.mdx` with a flat `key: value`
//! frontmatter block delimited by `---`. The shape is small enough that a YAML
//! crate would be more dependency than help: every line is one key, so a
//! line-by-line parse with an explicit key whitelist turns a typo into a build
//! error instead of a silently ignored field.

use anyhow::{Context, Result, bail};
use std::path::Path;

/// Posts shown on one blog index page.
pub const BLOG_PAGE_SIZE: usize = 10;

/// One blog post: its frontmatter fields plus the raw MDX body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    /// Publication time as epoch milliseconds, so sorting is a plain integer compare.
    pub pub_date: i64,
    pub updated_date: Option<i64>,
    pub featured_image: Option<String>,
    pub featured_image_alt: Option<String>,
    /// Everything after the closing `---`, untouched for the markdown stage.
    pub body: String,
}

/// Split a raw `.mdx` file into its frontmatter block and its body.
///
/// The opening delimiter must be the very first line. The closing delimiter is
/// the next line that is exactly `---`.
pub fn split_frontmatter(raw: &str) -> Result<(&str, &str)> {
    let raw = raw.strip_prefix("\u{feff}").unwrap_or(raw);
    let mut lines = raw.split_inclusive('\n');
    let Some(first) = lines.next() else {
        bail!("empty file: no frontmatter block");
    };
    if first.trim() != "---" {
        bail!("missing frontmatter: the file must start with `---`");
    }

    let mut offset = first.len();
    let mut frontmatter_end = None;
    for line in lines {
        if line.trim() == "---" {
            frontmatter_end = Some(offset);
            offset += line.len();
            break;
        }
        offset += line.len();
    }

    let Some(frontmatter_end) = frontmatter_end else {
        bail!("unterminated frontmatter: no closing `---`");
    };

    let frontmatter = &raw[first.len()..frontmatter_end];
    let body = &raw[offset..];
    Ok((frontmatter, body))
}

/// Strip one layer of matching quotes and surrounding whitespace.
fn unquote(value: &str) -> String {
    let value = value.trim();
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'\"' && last == b'\"') || (first == b'\'' && last == b'\'') {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn none_if_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

/// Parse the frontmatter block into a [`Post`]; `body` is left empty.
///
/// Unknown keys are an error: a mistyped `pubDatum` must fail the build rather
/// than silently publish an undated post.
pub fn parse_frontmatter(frontmatter: &str) -> Result<Post> {
    let mut slug = None;
    let mut title = None;
    let mut description = None;
    let mut category = None;
    let mut pub_date = None;
    let mut updated_date = None;
    let mut featured_image = None;
    let mut featured_image_alt = None;

    for (index, line) in frontmatter.lines().enumerate() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            bail!("frontmatter line {} is not `key: value`: {line}", index + 1);
        };
        let key = key.trim();
        let value = unquote(value);
        match key {
            "slug" => slug = none_if_empty(value),
            "title" => title = none_if_empty(value),
            "description" => description = none_if_empty(value),
            "blogCategory" => category = none_if_empty(value),
            "featuredImage" => featured_image = none_if_empty(value),
            "featuredImageAlt" => featured_image_alt = none_if_empty(value),
            "pubDate" => {
                let parsed = parse_date(&value).ok_or_else(|| {
                    anyhow::anyhow!("pubDate is not an RFC3339 timestamp: {value}")
                })?;
                pub_date = Some(parsed);
            }
            "updatedDate" => {
                let parsed = parse_date(&value).ok_or_else(|| {
                    anyhow::anyhow!("updatedDate is not an RFC3339 timestamp: {value}")
                })?;
                updated_date = Some(parsed);
            }
            other => bail!("unknown frontmatter key `{other}` on line {}", index + 1),
        }
    }

    let slug = slug.ok_or_else(|| anyhow::anyhow!("frontmatter is missing required key `slug`"))?;
    let title =
        title.ok_or_else(|| anyhow::anyhow!("frontmatter is missing required key `title`"))?;
    let pub_date =
        pub_date.ok_or_else(|| anyhow::anyhow!("frontmatter is missing required key `pubDate`"))?;

    Ok(Post {
        slug,
        title,
        description,
        category,
        pub_date,
        updated_date,
        featured_image,
        featured_image_alt,
        body: String::new(),
    })
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn parse_fixed<T: std::str::FromStr>(text: &str) -> Option<T> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Parse an RFC3339 timestamp into epoch milliseconds.
///
/// Accepts a `Z` designator or a numeric `+HH:MM` / `-HH:MM` offset, plus an
/// optional fractional-seconds part, which is truncated to whole milliseconds.
/// Returns `None` for anything malformed so callers can report the field.
pub fn parse_date(value: &str) -> Option<i64> {
    let text = value.trim();
    let (date, time) = text.split_once('T')?;

    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parse_fixed(parts.next()?)?;
    let day: i64 = parse_fixed(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    if time.len() < 8 {
        return None;
    }
    let hour: i64 = parse_fixed(time.get(0..2)?)?;
    if time.as_bytes()[2] != b':' {
        return None;
    }
    let minute: i64 = parse_fixed(time.get(3..5)?)?;
    if time.as_bytes()[5] != b':' {
        return None;
    }
    let second: i64 = parse_fixed(time.get(6..8)?)?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let mut rest = &time[8..];
    let mut millis = 0i64;
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        if digits == 0 {
            return None;
        }
        let mut scaled = 0i64;
        for (index, digit) in fraction[..digits].bytes().enumerate() {
            if index == 3 {
                break;
            }
            scaled = scaled * 10 + i64::from(digit - b'0');
        }
        for _ in digits.min(3)..3 {
            scaled *= 10;
        }
        millis = scaled;
        rest = &fraction[digits..];
    }

    let offset_seconds = match rest {
        "Z" | "z" => 0,
        other => {
            let (sign, tail) = match other.as_bytes().first()? {
                b'+' => (1i64, &other[1..]),
                b'-' => (-1i64, &other[1..]),
                _ => return None,
            };
            let (hours, minutes) = tail.split_once(':')?;
            let hours: i64 = parse_fixed(hours)?;
            let minutes: i64 = parse_fixed(minutes)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            sign * (hours * 3600 + minutes * 60)
        }
    };

    let days = days_from_civil(year, month, day);
    let seconds = hour * 3600 + minute * 60 + second - offset_seconds;
    Some((days * 86_400 + seconds) * 1000 + millis)
}

/// Load every post under `root`, newest first.
///
/// Directories without an `index.mdx` are skipped so shared asset folders can
/// live beside the posts without being treated as content.
pub fn load_posts(root: &Path) -> Result<Vec<Post>> {
    let entries = std::fs::read_dir(root)
        .with_context(|| format!("reading content directory {}", root.display()))?;

    let mut posts = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }
        let file = path.join("index.mdx");
        if !file.is_file() {
            continue;
        }
        let raw = std::fs::read_to_string(&file)
            .with_context(|| format!("reading {}", file.display()))?;
        let (frontmatter, body) =
            split_frontmatter(&raw).with_context(|| format!("parsing {}", file.display()))?;
        let mut post = parse_frontmatter(frontmatter)
            .with_context(|| format!("parsing {}", file.display()))?;
        post.body = body.trim().to_string();
        posts.push(post);
    }

    posts.sort_by(|a, b| {
        b.pub_date
            .cmp(&a.pub_date)
            .then_with(|| b.slug.cmp(&a.slug))
    });
    Ok(posts)
}

/// Number of index pages for `total` posts; an empty blog still has page 1.
pub fn max_pages(total: usize) -> usize {
    if total == 0 {
        1
    } else {
        total.div_ceil(BLOG_PAGE_SIZE)
    }
}

/// Canonical href for a blog index page: page 1 is `/blog`, the rest are paged.
pub fn page_href(page: usize) -> String {
    if page <= 1 {
        "/blog".to_string()
    } else {
        format!("/blog/page/{page}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixtures() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/blog")
    }

    #[test]
    fn loads_every_post_directory() {
        let posts = load_posts(&fixtures()).unwrap();
        assert_eq!(posts.len(), 2);
    }

    #[test]
    fn sorts_newest_first() {
        let posts = load_posts(&fixtures()).unwrap();
        assert_eq!(posts[0].slug, "beta");
        assert_eq!(posts[1].slug, "alpha");
    }

    #[test]
    fn reads_optional_fields() {
        let posts = load_posts(&fixtures()).unwrap();
        let alpha = posts.iter().find(|p| p.slug == "alpha").unwrap();
        assert_eq!(alpha.category.as_deref(), Some("Engineering"));
        assert_eq!(alpha.description.as_deref(), Some("First fixture post."));
        let beta = posts.iter().find(|p| p.slug == "beta").unwrap();
        assert!(beta.category.is_none());
    }

    #[test]
    fn body_excludes_frontmatter() {
        let posts = load_posts(&fixtures()).unwrap();
        assert!(posts.iter().all(|p| !p.body.contains("pubDate")));
    }

    #[test]
    fn pagination_helpers_match_the_old_site() {
        assert_eq!(max_pages(0), 1);
        assert_eq!(max_pages(10), 1);
        assert_eq!(max_pages(11), 2);
        assert_eq!(max_pages(37), 4);
        assert_eq!(page_href(1), "/blog");
        assert_eq!(page_href(2), "/blog/page/2");
    }

    #[test]
    fn parses_rfc3339_utc() {
        assert_eq!(parse_date("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_date("2024-01-02T00:00:00Z"), Some(1704153600000));
    }

    #[test]
    fn rejects_malformed_dates() {
        assert_eq!(parse_date("not a date"), None);
        assert_eq!(parse_date("2024-13-01T00:00:00Z"), None);
    }

    #[test]
    fn parses_timezone_offsets_and_fractions() {
        // Same instant, three spellings.
        assert_eq!(parse_date("2024-01-02T00:00:00Z"), Some(1704153600000));
        assert_eq!(parse_date("2024-01-02T02:00:00+02:00"), Some(1704153600000));
        assert_eq!(parse_date("2024-01-01T19:00:00-05:00"), Some(1704153600000));
        assert_eq!(parse_date("2024-01-02T00:00:00.250Z"), Some(1704153600250));
        assert_eq!(parse_date("2024-01-02T00:00:00.5Z"), Some(1704153600500));
    }

    #[test]
    fn accepts_unquoted_values_like_the_real_content() {
        let post = parse_frontmatter(
            "pubDate: 2024-01-02T00:00:00Z\nslug: alpha\nfeaturedImage: ./images/featured.jpg\ntitle: \"Alpha: A Post\"\n",
        )
        .unwrap();
        assert_eq!(post.slug, "alpha");
        assert_eq!(post.title, "Alpha: A Post");
        assert_eq!(
            post.featured_image.as_deref(),
            Some("./images/featured.jpg")
        );
        assert_eq!(post.pub_date, 1704153600000);
    }

    #[test]
    fn missing_required_key_is_an_error() {
        assert!(parse_frontmatter("pubDate: \"2024-01-02T00:00:00Z\"\nslug: \"alpha\"\n").is_err());
        assert!(
            parse_frontmatter("pubDate: \"2024-01-02T00:00:00Z\"\ntitle: \"Alpha\"\n").is_err()
        );
    }

    #[test]
    fn unknown_frontmatter_key_is_an_error() {
        let dir = std::env::temp_dir().join("ethang-frontmatter-typo");
        let post_dir = dir.join("bad");
        std::fs::create_dir_all(&post_dir).unwrap();
        std::fs::write(
            post_dir.join("index.mdx"),
            "---\npubDatum: \"2024-01-01T00:00:00Z\"\nslug: \"bad\"\ntitle: \"Bad\"\n---\n",
        )
        .unwrap();
        assert!(load_posts(&dir).is_err());
    }
}
