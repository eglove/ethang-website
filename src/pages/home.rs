//! The home page: profile card, operating principles, current toolkit, and the
//! selected work list.
//!
//! The two project lists come from `content/home.toml`; the rest is fixed copy
//! ported from the old Astro `index.astro`. [`HomeData`] carries only the
//! projects, so [`parse_home`] checks the shipped-work heading in the TOML
//! against the constant the page renders: editing the file can never silently
//! disagree with the page.

use crate::html::{el, escape, raw, void_el};
use anyhow::{Context, Result, bail};
use std::path::Path;

/// One shipped project: a name, a sentence about it, and the outcome when the
/// work unblocked something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub detail: String,
    pub unstick: Option<String>,
}

/// Everything the home page renders from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeData {
    pub client_projects: Vec<Project>,
    pub monorepo_projects: Vec<Project>,
}

/// The heading above the shipped-work grid. `content/home.toml` carries it as
/// `whatIveShippedHeading` (with a typographic apostrophe); [`parse_home`]
/// fails the build when the two drift.
pub const SHIPPED_HEADING: &str = "What I\u{2019}ve shipped";

const NAME: &str = "Ethan Glover";
const KICKER: &str = "software engineer / builder";
const TAGLINE: &str =
    "I build resilient software, clarify complex domains, and leave systems easier to change.";
const CALENDAR_URL: &str =
    "https://calendar.proton.me/u/2/bookings#10S2zo5jm_rTCXVDnyi55vEBS9HN8Cam4w3HxHr5Omg=";

/// The three operating principles: accent token, title, description.
const PRINCIPLES: [(&str, &str, &str); 3] = [
    (
        "night-owl-cyan",
        "Hypothesis-first.",
        "I write the failing test before the implementation. Red, green, refactor \u{2014} every PR, every feature, every bug fix.",
    ),
    (
        "night-owl-purple",
        "Grounded in SWEBOK.",
        "Estimates and tradeoffs grounded in the SWEBOK software engineering body of knowledge.",
    ),
    (
        "night-owl-green",
        "Domain-Driven Design as the bridge.",
        "I often promote Domain-Driven Design as the bridge between development and business understanding. The model is the conversation.",
    ),
];

const LANGUAGES: [&str; 10] = [
    "TypeScript",
    "JavaScript",
    "React",
    "Next.js",
    "React Native",
    "Solid",
    "Node.js",
    "C#/.NET",
    "Python",
    "SQL",
];

const STACK: [&str; 15] = [
    "TanStack Router",
    "TanStack Query",
    "Radix Themes",
    "Tailwind",
    "Cloudflare Workers",
    "Hono",
    "Drizzle ORM",
    "D1 (SQLite)",
    "React Query",
    "Redux",
    "GraphQL",
    "REST",
    "WebSockets",
    "Spring Boot",
    "Microservices",
];

const PRACTICES: [&str; 12] = [
    "TDD (vitest, Playwright)",
    "DDD",
    "Monorepo",
    "Effect-TS",
    "Lodash",
    "Dependency injection",
    "Composable design",
    "Accessibility",
    "SEO",
    "Performance budgeting",
    "Container-driven development",
    "Legacy migrations",
];

/// The Lucide `mail` glyph, inlined so the link row needs no icon font.
const MAIL_ICON: &str = concat!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\" viewBox=\"0 0 24 24\" ",
    "fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" ",
    "stroke-linejoin=\"round\" aria-hidden=\"true\"><rect width=\"20\" height=\"16\" x=\"2\" ",
    "y=\"4\" rx=\"2\"></rect><path d=\"m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7\"></path></svg>"
);

/// Read and parse `content/home.toml` from the content root.
pub fn load_home(root: &Path) -> Result<HomeData> {
    let path = root.join("home.toml");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    parse_home(&text).with_context(|| format!("parsing {}", path.display()))
}

/// Parse the home page data out of TOML text.
pub fn parse_home(text: &str) -> Result<HomeData> {
    let table: toml::Table = text.parse().context("home.toml is not valid TOML")?;
    let heading = table
        .get("whatIveShippedHeading")
        .and_then(|value| value.as_str())
        .context("home.toml is missing `whatIveShippedHeading`")?;
    if heading != SHIPPED_HEADING {
        bail!(
            "home.toml `whatIveShippedHeading` is {heading:?} but the page renders {SHIPPED_HEADING:?}"
        );
    }
    Ok(HomeData {
        client_projects: projects(&table, "clientProjects")?,
        monorepo_projects: projects(&table, "monorepoProjects")?,
    })
}

fn projects(table: &toml::Table, key: &str) -> Result<Vec<Project>> {
    let array = table
        .get(key)
        .with_context(|| format!("home.toml is missing `[[{key}]]`"))?
        .as_array()
        .with_context(|| format!("`{key}` is not an array of tables"))?;
    array
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let entry = entry
                .as_table()
                .with_context(|| format!("`{key}` entry {} is not a table", index + 1))?;
            let field = |name: &str| -> Result<String> {
                entry
                    .get(name)
                    .and_then(|value| value.as_str())
                    .map(str::to_string)
                    .with_context(|| {
                        format!("`{key}` entry {} is missing a string `{name}`", index + 1)
                    })
            };
            let unstick = match entry.get("unstuck") {
                None => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .with_context(|| {
                            format!("`{key}` entry {} has a non-string `unstuck`", index + 1)
                        })?
                        .to_string(),
                ),
            };
            Ok(Project {
                name: field("name")?,
                detail: field("detail")?,
                unstick,
            })
        })
        .collect()
}

/// Render the home page body. The site shell is [`crate::pages::render_doc`].
pub fn render(data: &HomeData) -> String {
    let sections = vec![
        profile_card(),
        el(
            "div",
            &[("class", "home-columns")],
            &[principles_card(), toolkit_card()],
        ),
        work_card(data),
    ];
    el("div", &[("class", "page home")], &sections)
}

fn profile_card() -> String {
    let photo = void_el(
        "img",
        &[
            ("class", "home-profile__photo"),
            ("src", "/profile.jpeg"),
            ("alt", NAME),
            ("width", "112"),
            ("height", "112"),
            ("loading", "eager"),
            ("decoding", "async"),
        ],
    );
    let intro = el(
        "div",
        &[],
        &[
            el("p", &[("class", "section-kicker")], &[escape(KICKER)]),
            el(
                "h1",
                &[("id", "profile-heading"), ("class", "home-profile__name")],
                &[escape(NAME)],
            ),
            el(
                "p",
                &[("class", "home-profile__tagline")],
                &[escape(TAGLINE)],
            ),
        ],
    );
    let identity = el(
        "div",
        &[("class", "home-profile__identity")],
        &[photo, intro],
    );
    el(
        "section",
        &[
            ("class", "surface-card home-profile"),
            ("aria-labelledby", "profile-heading"),
        ],
        &[identity, link_row()],
    )
}

fn link_row() -> String {
    // The visible glyph is the icon; the accessible name comes from text inside
    // the link rather than aria-label, because WCAG 2.5.3 requires a visible
    // label's text to appear in the accessible name, and an aria-label that
    // omits the visible "in" or "GH" fails it.
    let external = |href: &str, label: &str, glyph: String| {
        el(
            "a",
            &[
                ("class", "home-links__link"),
                ("href", href),
                ("target", "_blank"),
                ("rel", "noopener noreferrer"),
            ],
            &[glyph, label_text(label)],
        )
    };
    let badge = |text: &str| el("span", &[("aria-hidden", "true")], &[escape(text)]);
    let icon = |src: &str, alt: &str| {
        void_el(
            "img",
            &[
                ("src", src),
                ("alt", alt),
                // The PNGs are 48x48 and the stylesheet draws them at 32px, so
                // the source is denser than the slot. Declaring the intrinsic
                // size keeps the aspect ratio; drawing them wider than 48 would
                // upscale the file, which Lighthouse's image-size-responsive
                // audit flags.
                ("width", "48"),
                ("height", "48"),
                ("loading", "lazy"),
            ],
        )
    };

    let mut items = vec![
        external(
            "https://www.linkedin.com/in/ethan-glover/",
            "LinkedIn",
            badge("in"),
        ),
        external("https://github.com/eglove", "GitHub", badge("GH")),
        el(
            "a",
            &[
                ("class", "home-links__link"),
                ("href", "mailto:hello@ethang.email"),
            ],
            &[raw(MAIL_ICON), label_text("Email")],
        ),
        external(
            "https://frontendmasters.com/u/ethang/",
            "Frontend Masters",
            icon("/frontend-masters-icon.png", "Frontend Masters"),
        ),
        external(
            "https://app.pluralsight.com/profile/ethan-glover-e9",
            "Pluralsight",
            icon("/pluralsight-icon.png", "Pluralsight"),
        ),
    ];
    items.push(el(
        "a",
        &[
            ("class", "home-links__cta"),
            ("href", CALENDAR_URL),
            ("target", "_blank"),
            ("rel", "noopener noreferrer"),
        ],
        &[escape("Schedule a Meeting")],
    ));

    let entries: Vec<String> = items
        .into_iter()
        .map(|item| el("li", &[], &[item]))
        .collect();
    el("ul", &[("class", "home-links")], &entries)
}

/// The accessible name for an icon link: present for assistive technology,
/// invisible on screen, and never part of the visible label.
fn label_text(label: &str) -> String {
    el("span", &[("class", "visually-hidden")], &[escape(label)])
}

fn principles_card() -> String {
    let principles: Vec<String> = PRINCIPLES
        .iter()
        .map(|(tone, title, body)| {
            let class = format!("home-principle home-principle--{tone}");
            el(
                "div",
                &[("class", class.as_str())],
                &[
                    el("p", &[("class", "home-principle__title")], &[escape(title)]),
                    el("p", &[("class", "home-principle__body")], &[escape(body)]),
                ],
            )
        })
        .collect();
    card(
        "01 / operating principles",
        "How I work",
        el("div", &[("class", "stack-list")], &principles),
        "home-principles",
    )
}

fn toolkit_card() -> String {
    let rows = [
        ("Languages & frameworks", LANGUAGES.join(", ")),
        ("Stack & infrastructure", STACK.join(", ")),
        ("Practices", PRACTICES.join(", ")),
    ];
    let groups: Vec<String> = rows
        .iter()
        .map(|(label, value)| {
            el(
                "div",
                &[],
                &[
                    el("span", &[("class", "section-kicker")], &[escape(label)]),
                    el("p", &[("class", "stack-list__value")], &[escape(value)]),
                ],
            )
        })
        .collect();
    card(
        "02 / current toolkit",
        "Stack & tools",
        el("div", &[("class", "stack-list")], &groups),
        "home-toolkit",
    )
}

fn card(kicker: &str, heading: &str, body: String, modifier: &str) -> String {
    let class = format!("card {modifier}");
    el(
        "section",
        &[("class", class.as_str())],
        &[
            el("p", &[("class", "section-kicker")], &[escape(kicker)]),
            el("h2", &[], &[escape(heading)]),
            body,
        ],
    )
}

fn work_card(data: &HomeData) -> String {
    // The old page hardcoded `13 client projects / 5 internal builds`. Deriving
    // both counts keeps the label honest when the TOML changes.
    let count = format!(
        "{} client projects / {} internal builds",
        data.client_projects.len(),
        data.monorepo_projects.len()
    );
    let head = el(
        "div",
        &[("class", "home-work__head")],
        &[
            el(
                "div",
                &[],
                &[
                    el(
                        "p",
                        &[("class", "section-kicker")],
                        &[escape("03 / selected work")],
                    ),
                    el("h2", &[], &[escape(SHIPPED_HEADING)]),
                ],
            ),
            el("span", &[("class", "home-work__count")], &[escape(&count)]),
        ],
    );
    let grid: Vec<String> = data
        .client_projects
        .iter()
        .chain(data.monorepo_projects.iter())
        .map(project_card)
        .collect();
    el(
        "section",
        &[("class", "card home-work")],
        &[head, el("div", &[("class", "home-work__grid")], &grid)],
    )
}

fn project_card(project: &Project) -> String {
    let mut children = vec![
        el(
            "p",
            &[("class", "home-project__name")],
            &[escape(&project.name)],
        ),
        el(
            "p",
            &[("class", "home-project__detail")],
            &[escape(&project.detail)],
        ),
    ];
    if let Some(unstick) = &project.unstick {
        children.push(el(
            "p",
            &[("class", "home-project__unstick")],
            &[escape(&format!("\u{2192} {unstick}"))],
        ));
    }
    el("div", &[("class", "home-project")], &children)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> HomeData {
        HomeData {
            client_projects: vec![Project {
                name: "Acme".into(),
                detail: "Built it.".into(),
                unstick: Some("Unblocked the team.".into()),
            }],
            monorepo_projects: vec![Project {
                name: "Site".into(),
                detail: "This site.".into(),
                unstick: None,
            }],
        }
    }

    #[test]
    fn renders_the_name_and_tagline() {
        let html = render(&data());
        assert!(html.contains("Ethan Glover"));
        assert!(html.contains("resilient software"));
    }

    #[test]
    fn renders_each_project_with_its_detail() {
        let html = render(&data());
        assert!(html.contains("Acme"));
        assert!(html.contains("Built it."));
        assert!(html.contains("Site"));
    }

    #[test]
    fn renders_the_unstick_line_only_when_present() {
        let html = render(&data());
        assert!(html.contains("Unblocked the team."));
        assert_eq!(html.matches("unstick").count(), 1);
    }

    #[test]
    fn escapes_project_text() {
        let mut d = data();
        d.client_projects[0].name = "A & B".into();
        assert!(render(&d).contains("A &amp; B"));
    }

    #[test]
    fn counts_are_derived_from_the_data() {
        let html = render(&data());
        assert!(html.contains("1 client projects / 1 internal builds"));
    }

    #[test]
    fn parse_reads_the_shipped_home_toml() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("content/home.toml"),
        )
        .unwrap();
        let data = parse_home(&text).unwrap();
        assert_eq!(data.client_projects.len(), 13);
        assert_eq!(data.monorepo_projects.len(), 5);
        assert_eq!(
            data.client_projects
                .iter()
                .filter(|project| project.unstick.is_some())
                .count(),
            3
        );
    }

    #[test]
    fn parse_rejects_a_mismatched_shipped_heading() {
        let text = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("content/home.toml"),
        )
        .unwrap()
        .replace(
            "whatIveShippedHeading = 'What I\u{2019}ve shipped'",
            "whatIveShippedHeading = 'Shipped'",
        );
        let error = parse_home(&text).unwrap_err().to_string();
        assert!(error.contains("whatIveShippedHeading"), "{error}");
    }
}
