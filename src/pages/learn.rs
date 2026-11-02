//! The Learn page: the ten-stage curriculum carried in `content/learn.toml`.
//!
//! Ported from the old Astro site - `learn.astro` for the page, `lib/learn.ts`
//! for the rules, and `MarkerBadge.astro` for the six Lucide glyphs, which are
//! inlined as paths so a badge needs no icon font and no client dependency.
//!
//! The page renders structure, not prose: every lede, blurb, note, and check is
//! data. [`parse_learn`] validates that structure - known markers, unique slugs
//! and numbers, cross-references that resolve - so a typo in the TOML fails the
//! build instead of rendering a badge with no glyph.

use crate::html::{el, escape, void_el};
use crate::pages::{Doc, SITE};
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::path::Path;

/// One stage of the curriculum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    pub number: u8,
    pub slug: String,
    pub title: String,
    pub lede: String,
    pub done_when: String,
    pub pair_with: String,
    pub groups: Vec<Group>,
}

/// A run of entries under one optional heading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub heading: Option<String>,
    pub blurb: Option<String>,
    pub entries: Vec<Entry>,
}

/// One source: how it is consumed, what it is, and what it is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub marker: String,
    pub title: String,
    pub note: String,
    pub by: Option<String>,
    pub url: Option<String>,
    /// Stage numbers this entry is also filed under, for cross-references.
    pub also_in: Vec<u8>,
}

/// The intro paragraph, verbatim from the old page.
pub const INTRO: &str = "The material that teaches the part a model cannot do for you: deciding what to build, specifying it so a machine can execute it, and checking the result. Work the stages in order, or jump to the one you are missing. Every stage mixes something you can follow along with and something worth returning to, and ends with a check you can run to know it landed.";

/// The six markers, in the order the legend lists them.
pub const MARKERS: [&str; 6] = ["video", "course", "book", "lab", "paper", "corpus"];

/// The register each marker arrives in, in the list's own words.
pub const MARKER_REGISTERS: [(&str, &str); 6] = [
    (
        "video",
        "Watch and follow along. Short episodes, high production, low friction.",
    ),
    (
        "course",
        "Structured series with exercises, longer commitment.",
    ),
    ("book", "Read, with a voice rather than a specification."),
    ("lab", "Interactive, you break something on purpose."),
    ("paper", "Primary source, dense, worth the effort."),
    (
        "corpus",
        "Worked examples to read rather than a thing to study.",
    ),
];

/// Lucide geometry, inlined as paths.
///
/// Importing an icon package into the old build broke its prerender chunking,
/// and six static icons do not justify a client dependency: the markup is a
/// 24x24 stroke icon either way. Paths from lucide 0.556.0.
pub const MARKER_PATHS: [(&str, &[&str]); 6] = [
    (
        "video",
        &["M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z"],
    ),
    (
        "course",
        &[
            "M21.42 10.922a1 1 0 0 0-.019-1.838L12.83 5.18a2 2 0 0 0-1.66 0L2.6 9.08a1 1 0 0 0 0 1.832l8.57 3.908a2 2 0 0 0 1.66 0z",
            "M22 10v6",
            "M6 12.5V16a6 3 0 0 0 12 0v-3.5",
        ],
    ),
    (
        "book",
        &[
            "M12 7v14",
            "M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z",
        ],
    ),
    (
        "lab",
        &[
            "M14 2v6a2 2 0 0 0 .245.96l5.51 10.08A2 2 0 0 1 18 22H6a2 2 0 0 1-1.755-2.96l5.51-10.08A2 2 0 0 0 10 8V2",
            "M6.453 15h11.094",
            "M8.5 2h7",
        ],
    ),
    (
        "paper",
        &[
            "M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z",
            "M14 2v5a1 1 0 0 0 1 1h5",
            "M10 9H8",
            "M16 13H8",
            "M16 17H8",
        ],
    ),
    ("corpus", &["m16 6 4 14", "M12 6v14", "M8 8v12", "M4 4v16"]),
];

/// The zero-padded stage number: `01` through `10`.
pub fn format_stage_number(number: u8) -> String {
    format!("{number:02}")
}

/// `1 entry` / `N entries`.
pub fn entry_count_label(count: usize) -> String {
    let noun = if count == 1 { "entry" } else { "entries" };
    format!("{count} {noun}")
}

/// Every entry in the stage, in group order.
pub fn stage_entries(stage: &Stage) -> impl Iterator<Item = &Entry> {
    stage.groups.iter().flat_map(|group| group.entries.iter())
}

/// How many entries the whole curriculum carries.
pub fn total_entries(stages: &[Stage]) -> usize {
    stages
        .iter()
        .map(|stage| stage_entries(stage).count())
        .sum::<usize>()
}

/// The markers a stage uses, with how many of each, in register order.
///
/// Returned as data rather than a formatted string so the page can render each
/// as its own tally badge.
pub fn stage_marker_summary(stage: &Stage) -> Vec<(&'static str, usize)> {
    MARKERS
        .iter()
        .filter_map(|marker| {
            let count = stage_entries(stage)
                .filter(|entry| entry.marker == *marker)
                .count();
            if count == 0 {
                None
            } else {
                Some((*marker, count))
            }
        })
        .collect()
}

/// The Lucide paths for one marker; empty for an unknown one.
fn marker_paths(marker: &str) -> &'static [&'static str] {
    match MARKER_PATHS.iter().find(|(name, _)| *name == marker) {
        Some((_, paths)) => paths,
        None => &[],
    }
}

/// Render the `<main>` body of the Learn page.
pub fn render(stages: &[Stage]) -> String {
    let sections: Vec<String> = stages
        .iter()
        .map(|stage| stage_section(stage, stages))
        .collect();
    el(
        "div",
        &[("class", "page learn")],
        &[
            head(),
            intro(),
            stage_map(stages),
            legend(),
            tally(stages),
            el("div", &[("class", "learn-stages")], &sections),
        ],
    )
}

/// The complete Learn page, ready for [`crate::pages::render_doc`].
pub fn learn_doc(stages: &[Stage]) -> Doc {
    Doc {
        title: "Learn".into(),
        description: Some(INTRO.into()),
        canonical: Some(format!("{SITE}/learn")),
        article: None,
        body: render(stages),
    }
}

fn head() -> String {
    el(
        "div",
        &[("class", "learn-head")],
        &[
            el(
                "p",
                &[("class", "section-kicker")],
                &[escape("what to learn / in what order")],
            ),
            el("h1", &[("class", "learn-head__title")], &[escape("Learn")]),
            el("div", &[("class", "section-rule")], &[]),
        ],
    )
}

fn intro() -> String {
    el("p", &[("class", "learn-intro")], &[escape(INTRO)])
}

/// The map first: ten stages, what each is for, and how much is in it.
fn stage_map(stages: &[Stage]) -> String {
    let items: Vec<String> = stages
        .iter()
        .map(|stage| {
            let href = format!("#{}", stage.slug);
            let link = el(
                "a",
                &[("class", "learn-map__link"), ("href", href.as_str())],
                &[
                    el(
                        "span",
                        &[("class", "learn-map__number")],
                        &[escape(&format_stage_number(stage.number))],
                    ),
                    el(
                        "span",
                        &[("class", "learn-map__title")],
                        &[escape(&stage.title)],
                    ),
                    el(
                        "span",
                        &[("class", "learn-map__count")],
                        &[escape(&entry_count_label(stage_entries(stage).count()))],
                    ),
                ],
            );
            el("li", &[("class", "learn-map__item")], &[link])
        })
        .collect();
    el(
        "nav",
        &[("class", "learn-map"), ("aria-label", "Stages")],
        &[el("ol", &[("class", "learn-map__list")], &items)],
    )
}

/// The formats are labelled once, where a reader first meets them, rather than
/// explained at every badge.
fn legend() -> String {
    let items: Vec<String> = MARKER_REGISTERS
        .iter()
        .map(|(marker, register)| {
            el(
                "div",
                &[("class", "learn-legend__item")],
                &[
                    el("dt", &[], &[badge(marker, None, 14)]),
                    el(
                        "dd",
                        &[("class", "learn-legend__register")],
                        &[escape(register)],
                    ),
                ],
            )
        })
        .collect();
    el("dl", &[("class", "learn-legend")], &items)
}

fn tally(stages: &[Stage]) -> String {
    let line = format!(
        "{} stages / {} sources",
        stages.len(),
        total_entries(stages)
    );
    el("p", &[("class", "learn-tally")], &[escape(&line)])
}

fn stage_section(stage: &Stage, stages: &[Stage]) -> String {
    let markers: Vec<String> = stage_marker_summary(stage)
        .iter()
        .map(|(marker, count)| badge(marker, Some(*count), 12))
        .collect();
    let head = el(
        "div",
        &[("class", "learn-stage__head")],
        &[
            el(
                "span",
                &[("class", "learn-stage__number")],
                &[escape(&format_stage_number(stage.number))],
            ),
            el(
                "h2",
                &[("class", "learn-stage__title")],
                &[escape(&stage.title)],
            ),
            el("span", &[("class", "learn-stage__markers")], &markers),
        ],
    );
    let groups: Vec<String> = stage
        .groups
        .iter()
        .map(|group| group_card(group, stages))
        .collect();
    let done = el(
        "div",
        &[("class", "learn-done")],
        &[
            el("p", &[("class", "section-kicker")], &[escape("Done when")]),
            el(
                "p",
                &[("class", "learn-done__text")],
                &[escape(&stage.done_when)],
            ),
        ],
    );
    let pair = el(
        "p",
        &[("class", "learn-pair")],
        &[
            el(
                "span",
                &[("class", "learn-pair__label")],
                &[escape("Pair it with")],
            ),
            escape(" "),
            escape(&stage.pair_with),
        ],
    );
    el(
        "section",
        &[("id", stage.slug.as_str()), ("class", "learn-stage")],
        &[
            head,
            el("div", &[("class", "section-rule")], &[]),
            el(
                "p",
                &[("class", "learn-stage__lede")],
                &[escape(&stage.lede)],
            ),
            el("div", &[("class", "learn-groups")], &groups),
            done,
            pair,
        ],
    )
}

fn group_card(group: &Group, stages: &[Stage]) -> String {
    let mut children = Vec::new();
    if let Some(heading) = &group.heading {
        children.push(el(
            "h3",
            &[("class", "learn-group__heading")],
            &[escape(heading)],
        ));
    }
    if let Some(blurb) = &group.blurb {
        children.push(el(
            "p",
            &[("class", "learn-group__blurb")],
            &[escape(blurb)],
        ));
    }
    let items: Vec<String> = group
        .entries
        .iter()
        .map(|entry| entry_item(entry, stages))
        .collect();
    children.push(el("ul", &[("class", "learn-entries")], &items));
    el("div", &[("class", "card learn-group")], &children)
}

fn entry_item(entry: &Entry, stages: &[Stage]) -> String {
    let mut head = vec![badge(&entry.marker, None, 14)];
    let title = match &entry.url {
        Some(url) => el(
            "a",
            &[
                ("class", "learn-entry__link"),
                ("href", url.as_str()),
                ("target", "_blank"),
                ("rel", "noopener noreferrer"),
            ],
            &[escape(&entry.title)],
        ),
        None => escape(&entry.title),
    };
    head.push(el("span", &[("class", "learn-entry__title")], &[title]));
    if let Some(by) = &entry.by {
        head.push(el("span", &[("class", "learn-entry__by")], &[escape(by)]));
    }
    for number in &entry.also_in {
        if let Some(other) = stages.iter().find(|candidate| candidate.number == *number) {
            let href = format!("#{}", other.slug);
            let label = format!("also in {}", format_stage_number(*number));
            head.push(el(
                "a",
                &[("class", "learn-entry__also"), ("href", href.as_str())],
                &[escape(&label)],
            ));
        }
    }
    el(
        "li",
        &[("class", "learn-entry")],
        &[
            el("div", &[("class", "learn-entry__head")], &head),
            el(
                "p",
                &[("class", "learn-entry__note")],
                &[escape(&entry.note)],
            ),
        ],
    )
}

/// A marker badge: the icon is the badge, and the format name is carried as an
/// `aria-label` on a `role="img"` wrapper, so a screen reader announces
/// "video" while a sighted reader gets the glyph and no visible label competes
/// with it. A tally badge adds the count beside the icon and says so.
fn badge(marker: &str, count: Option<usize>, size: u8) -> String {
    let label = match count {
        Some(count) => format!("{count} {marker}"),
        None => marker.to_string(),
    };
    let class = format!("marker-badge marker-badge--{marker}");
    let mut children = vec![badge_icon(marker, size)];
    if let Some(count) = count {
        children.push(el(
            "span",
            &[("class", "marker-badge__count")],
            &[escape(&count.to_string())],
        ));
    }
    el(
        "span",
        &[
            ("role", "img"),
            ("aria-label", label.as_str()),
            ("class", class.as_str()),
        ],
        &children,
    )
}

fn badge_icon(marker: &str, size: u8) -> String {
    let size = size.to_string();
    let attrs = [
        ("xmlns", "http://www.w3.org/2000/svg"),
        ("width", size.as_str()),
        ("height", size.as_str()),
        ("viewBox", "0 0 24 24"),
        ("fill", "none"),
        ("stroke", "currentColor"),
        ("stroke-width", "2"),
        ("stroke-linecap", "round"),
        ("stroke-linejoin", "round"),
        ("aria-hidden", "true"),
        ("class", "marker-badge__icon"),
    ];
    let paths: Vec<String> = marker_paths(marker)
        .iter()
        .map(|d| void_el("path", &[("d", d)]))
        .collect();
    el("svg", &attrs, &paths)
}

/// Read and parse `content/learn.toml` from the content root.
pub fn load_learn(root: &Path) -> Result<Vec<Stage>> {
    let path = root.join("learn.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse_learn(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Parse the curriculum out of TOML text.
pub fn parse_learn(text: &str) -> Result<Vec<Stage>> {
    let table: toml::Table = text.parse().context("learn.toml is not valid TOML")?;
    let stages = table
        .get("stages")
        .and_then(toml::Value::as_array)
        .context("learn.toml is missing `stages`")?;
    let stages: Vec<Stage> = stages.iter().map(parse_stage).collect::<Result<_>>()?;
    validate(&stages)?;
    Ok(stages)
}

fn parse_stage(value: &toml::Value) -> Result<Stage> {
    let table = value
        .as_table()
        .context("a `[[stages]]` entry is not a table")?;
    let number =
        u8::try_from(integer(table, "number")?).context("a stage number does not fit in a u8")?;
    let groups = match table.get("groups").and_then(toml::Value::as_array) {
        Some(groups) => groups.iter().map(parse_group).collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    Ok(Stage {
        number,
        slug: string(table, "slug")?,
        title: string(table, "title")?,
        lede: string(table, "lede")?,
        done_when: string(table, "doneWhen")?,
        pair_with: string(table, "pairWith")?,
        groups,
    })
}

fn parse_group(value: &toml::Value) -> Result<Group> {
    let table = value
        .as_table()
        .context("a `[[stages.groups]]` entry is not a table")?;
    let entries = match table.get("entries").and_then(toml::Value::as_array) {
        Some(entries) => entries
            .iter()
            .map(parse_entry)
            .collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    Ok(Group {
        heading: optional_string(table, "heading"),
        blurb: optional_string(table, "blurb"),
        entries,
    })
}

fn parse_entry(value: &toml::Value) -> Result<Entry> {
    let table = value
        .as_table()
        .context("a `[[stages.groups.entries]]` entry is not a table")?;
    let also_in = match table.get("alsoIn").and_then(toml::Value::as_array) {
        Some(numbers) => numbers
            .iter()
            .map(|number| {
                let number = number
                    .as_integer()
                    .context("`alsoIn` holds a non-integer")?;
                u8::try_from(number).context("an `alsoIn` number does not fit in a u8")
            })
            .collect::<Result<Vec<_>>>()?,
        None => Vec::new(),
    };
    Ok(Entry {
        marker: string(table, "marker")?,
        title: string(table, "title")?,
        note: string(table, "note")?,
        by: optional_string(table, "by"),
        url: optional_string(table, "url"),
        also_in,
    })
}

fn string(table: &toml::Table, key: &str) -> Result<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("missing string `{key}`"))
}

fn optional_string(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

fn integer(table: &toml::Table, key: &str) -> Result<i64> {
    table
        .get(key)
        .and_then(toml::Value::as_integer)
        .with_context(|| format!("missing integer `{key}`"))
}

/// Fail the build on data the page cannot render honestly: an unknown marker
/// has no glyph, a duplicate slug has no anchor, and a cross-reference to a
/// stage that does not exist would silently render nothing.
fn validate(stages: &[Stage]) -> Result<()> {
    let numbers: Vec<u8> = stages.iter().map(|stage| stage.number).collect();
    let mut slugs: BTreeSet<&str> = BTreeSet::new();
    for stage in stages {
        if stage.slug.trim().is_empty() {
            bail!("stage {} has an empty slug", stage.number);
        }
        if !slugs.insert(stage.slug.as_str()) {
            bail!("duplicate stage slug `{}`", stage.slug);
        }
        for entry in stage_entries(stage) {
            if !MARKERS.contains(&entry.marker.as_str()) {
                bail!(
                    "stage {} entry `{}` has unknown marker `{}`",
                    stage.number,
                    entry.title,
                    entry.marker
                );
            }
            for number in &entry.also_in {
                if !numbers.contains(number) {
                    bail!(
                        "stage {} entry `{}` cross-references missing stage {number}",
                        stage.number,
                        entry.title
                    );
                }
            }
        }
    }
    if numbers.iter().copied().collect::<BTreeSet<u8>>().len() != numbers.len() {
        bail!("two stages share a number");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stage() -> Stage {
        Stage {
            number: 1,
            slug: "fluency".into(),
            title: "Fluency".into(),
            lede: "The unglamorous prerequisites.".into(),
            done_when: "You can start a project.".into(),
            pair_with: "Watch the series in sequence.".into(),
            groups: vec![Group {
                heading: Some("The Hard Parts, in full".into()),
                blurb: None,
                entries: vec![Entry {
                    marker: "video".into(),
                    title: "Missing Semester".into(),
                    note: "Best hour-to-value ratio.".into(),
                    by: Some("MIT".into()),
                    url: Some("https://missing.csail.mit.edu/".into()),
                    also_in: vec![8],
                }],
            }],
        }
    }

    #[test]
    fn renders_a_nav_entry_per_stage() {
        let html = render(&[stage()]);
        assert!(html.contains("href=\"#fluency\""));
        assert!(html.contains("01"));
    }

    #[test]
    fn renders_entries_with_links_and_notes() {
        let html = render(&[stage()]);
        assert!(html.contains("https://missing.csail.mit.edu/"));
        assert!(html.contains("Best hour-to-value ratio."));
        assert!(html.contains("MIT"));
    }

    #[test]
    fn renders_also_in_cross_links() {
        let mut s = stage();
        s.groups[0].entries[0].also_in = vec![8];
        let mut other = stage();
        other.number = 8;
        other.slug = "ai-engineering".into();
        let html = render(&[s, other]);
        assert!(html.contains("href=\"#ai-engineering\""));
        assert!(html.contains("also in 08"));
    }

    #[test]
    fn renders_done_when_and_pair_with() {
        let html = render(&[stage()]);
        assert!(html.contains("You can start a project."));
        assert!(html.contains("Watch the series in sequence."));
    }

    #[test]
    fn renders_a_badge_per_marker() {
        let html = render(&[stage()]);
        assert!(html.contains("aria-label=\"video\""));
    }

    #[test]
    fn counts_entries_across_stages() {
        assert_eq!(total_entries(&[stage(), stage()]), 2);
    }

    #[test]
    fn stage_numbers_are_zero_padded() {
        assert_eq!(format_stage_number(1), "01");
        assert_eq!(format_stage_number(10), "10");
    }

    #[test]
    fn entry_count_labels_singular_and_plural() {
        assert_eq!(entry_count_label(1), "1 entry");
        assert_eq!(entry_count_label(97), "97 entries");
        assert_eq!(entry_count_label(0), "0 entries");
    }

    #[test]
    fn legend_labels_every_marker_once() {
        let html = render(&[stage()]);
        for marker in MARKERS {
            assert!(
                html.contains(&format!("aria-label=\"{marker}\"")),
                "missing legend badge for {marker}"
            );
        }
        assert!(html.contains("Worked examples to read rather than a thing to study."));
    }

    #[test]
    fn tally_counts_stages_and_sources() {
        let html = render(&[stage(), stage()]);
        assert!(html.contains("2 stages / 2 sources"));
    }

    #[test]
    fn parses_the_shipped_curriculum() {
        let stages = load_learn(&crate::pages::content_root()).unwrap();
        assert_eq!(stages.len(), 10);
        assert_eq!(total_entries(&stages), 97);
        assert_eq!(stages[0].slug, "fluency");
        assert_eq!(stages[9].number, 10);
        let html = render(&stages);
        assert!(html.contains("10 stages / 97 sources"));
        assert!(html.contains("id=\"the-primary-record\""));
    }

    #[test]
    fn rejects_an_unknown_marker() {
        let text = "[[stages]]\nnumber = 1\nslug = 'x'\ntitle = 'X'\nlede = 'l'\ndoneWhen = 'd'\npairWith = 'p'\n\n[[stages.groups]]\n\n[[stages.groups.entries]]\nmarker = 'podcast'\ntitle = 'T'\nnote = 'n'\n";
        let err = parse_learn(text).unwrap_err();
        assert!(format!("{err:#}").contains("unknown marker"));
    }

    #[test]
    fn rejects_a_cross_reference_to_a_missing_stage() {
        let text = "[[stages]]\nnumber = 1\nslug = 'x'\ntitle = 'X'\nlede = 'l'\ndoneWhen = 'd'\npairWith = 'p'\n\n[[stages.groups]]\n\n[[stages.groups.entries]]\nmarker = 'video'\ntitle = 'T'\nnote = 'n'\nalsoIn = [9]\n";
        let err = parse_learn(text).unwrap_err();
        assert!(format!("{err:#}").contains("missing stage 9"));
    }

    #[test]
    fn the_doc_carries_the_page_metadata() {
        let doc = learn_doc(&[stage()]);
        assert_eq!(doc.title, "Learn");
        assert_eq!(doc.canonical.as_deref(), Some("https://ethang.dev/learn"));
        assert_eq!(doc.description.as_deref(), Some(INTRO));
        assert!(doc.article.is_none());
    }
}
