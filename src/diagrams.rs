//! Mermaid fences resolved to committed SVG diagrams at build time.
//!
//! Mermaid cannot be rendered here. Its renderer is a JavaScript library, and
//! pulling it in would put JavaScript in the toolchain and a script on the
//! page, which is the one thing this rewrite exists to avoid. The 13 diagrams
//! in the corpus are therefore rendered once, outside this build, and the
//! resulting SVG is committed to `static/diagrams/`.
//!
//! Mermaid source carries no stable identifier, so a fence is matched to its
//! diagram by a hash of the trimmed source. That is deliberately exact: edit a
//! diagram's source and the hash stops matching, the fence falls back to its
//! visible source, and the build says so on stderr. A diagram that is visibly
//! out of date is a smaller problem than one that is silently wrong.

use crate::html::{el, escape, void_el};

/// Where the committed diagrams are published.
///
/// `static/diagrams/` is copied to the output root by the site build, so a file
/// committed there is served at this prefix with no rewriting in between.
pub const DIAGRAM_ROOT: &str = "/diagrams";

/// One committed diagram: the hash of the mermaid source it renders, the file
/// it is committed as, its intrinsic size, and the accessible name the
/// source's `accTitle` carried.
struct Diagram {
    hash: u64,
    file: &'static str,
    width: u32,
    height: u32,
    title: &'static str,
}

/// Every mermaid fence in the corpus, paired with its rendered SVG.
///
/// Re-rendering a diagram means replacing the file and the hash together;
/// `tests/diagrams.rs` fails when the two drift apart.
const DIAGRAMS: [Diagram; 13] = [
    Diagram {
        hash: 0x5f73_d6ca_c757_ab26,
        file: "the-2026-lab-incident-record.svg",
        width: 1207,
        height: 555,
        title: "The 2026 lab incident record",
    },
    Diagram {
        hash: 0x945b_13ca_99a4_be7a,
        file: "the-bounded-context-dependency-graph.svg",
        width: 1395,
        height: 786,
        title: "The bounded-context dependency graph",
    },
    Diagram {
        hash: 0x26bc_76a8_1088_7cfb,
        file: "one-root-turn-end-to-end.svg",
        width: 1425,
        height: 666,
        title: "One root turn, end to end",
    },
    Diagram {
        hash: 0x0450_7f8d_3077_d4b0,
        file: "the-sub-agent-system-from-spawn-to-supervision.svg",
        width: 657,
        height: 588,
        title: "The sub-agent system, from spawn to supervision",
    },
    Diagram {
        hash: 0xa77b_300c_89bc_e315,
        file: "the-agent-record-lifecycle.svg",
        width: 388,
        height: 320,
        title: "The agent record lifecycle",
    },
    Diagram {
        hash: 0x73ac_c471_0d0a_cb4e,
        file: "thin-slice-experiment-ratings.svg",
        width: 631,
        height: 195,
        title: "Thin-slice experiment ratings",
    },
    Diagram {
        hash: 0xdb48_59e8_9459_2235,
        file: "information-transfer-chains.svg",
        width: 1166,
        height: 467,
        title: "Information transfer chains",
    },
    Diagram {
        hash: 0x854f_39d3_b560_ee56,
        file: "five-defaults-and-their-adjustments.svg",
        width: 643,
        height: 609,
        title: "Five defaults and their adjustments",
    },
    Diagram {
        hash: 0x0331_e34f_8ec0_6e5c,
        file: "the-masking-burnout-loop.svg",
        width: 356,
        height: 692,
        title: "The masking burnout loop",
    },
    Diagram {
        hash: 0x9e27_2d07_ce52_0658,
        file: "feffermans-four-scenarios.svg",
        width: 1117,
        height: 327,
        title: "Fefferman's four scenarios",
    },
    Diagram {
        hash: 0x61f9_f7cb_fd1a_c0ea,
        file: "the-road-to-blowup.svg",
        width: 996,
        height: 608,
        title: "The road to blowup",
    },
    Diagram {
        hash: 0x820e_a7f4_b6cd_5778,
        file: "september-2025-to-september-2026.svg",
        width: 499,
        height: 1006,
        title: "September 2025 to September 2026",
    },
    Diagram {
        hash: 0x169c_3afa_cc0a_bc00,
        file: "who-answers-for-the-evaluation-layer.svg",
        width: 866,
        height: 644,
        title: "Who answers for the evaluation layer",
    },
];

/// Render a mermaid fence as the committed diagram it maps to.
///
/// The result is a complete `<figure class="diagram">` holding the SVG. The SVG
/// is referenced rather than inlined: mermaid's output namespaces its gradients,
/// markers, and CSS rules on its own `mermaid-<n>` id, and two of those inlined
/// on one page would collide on that id. As a separate document the SVG keeps
/// its ids to itself, which is also what lets it be cached on its own.
///
/// A source that matches no committed diagram degrades to
/// `<pre class="mermaid">` holding the escaped source, so a diagram that needs
/// re-rendering is visibly wrong in the output instead of silently absent.
pub fn render_mermaid(source: &str) -> String {
    let trimmed = source.trim();
    let Some(diagram) = DIAGRAMS.iter().find(|entry| entry.hash == fnv1a(trimmed)) else {
        eprintln!(
            "diagram: no committed SVG matches this mermaid source. Re-render it and \
             update `DIAGRAMS` in src/diagrams.rs:\n{trimmed}"
        );
        return fallback(trimmed);
    };

    let (width, height) = (diagram.width.to_string(), diagram.height.to_string());
    el(
        "figure",
        &[("class", "diagram")],
        &[void_el(
            "img",
            &[
                ("src", diagram_href(diagram.file).as_str()),
                ("alt", diagram.title),
                ("width", width.as_str()),
                ("height", height.as_str()),
                ("loading", "lazy"),
                ("decoding", "async"),
            ],
        )],
    )
}

/// The published URL for a committed diagram.
pub fn diagram_href(file: &str) -> String {
    format!("{DIAGRAM_ROOT}/{file}")
}

/// How many mermaid sources this build knows how to render.
pub fn committed_diagrams() -> usize {
    DIAGRAMS.len()
}

/// The visible stand-in for a fence with no committed diagram.
fn fallback(source: &str) -> String {
    el(
        "pre",
        &[("class", "mermaid")],
        &[el("code", &[], &[escape(source)])],
    )
}

/// FNV-1a over the UTF-8 bytes of `source`.
///
/// FNV-1a rather than a cryptographic digest: this only has to separate 13
/// fixed strings and be reproducible from a comment, and the constant is short
/// enough to recompute by hand if a diagram ever has to be re-keyed.
fn fnv1a(source: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One committed source, verbatim: `the-blowup-half`'s first fence.
    const KNOWN: &str = "---\naccTitle: Fefferman's four scenarios\n---\nflowchart TD\n    root[\"Fefferman's four alternatives\"] --> a[\"(A) unbounded space, no force:<br/>smooth data stay smooth\"]\n    root --> b[\"(B) periodic box, no force\"]\n    root --> c[\"(C) unbounded space, smooth force:<br/>the solution breaks down\"]\n    root --> d[\"(D) periodic box, smooth force\"]\n    c --> claimed[\"claimed by the OpenAI paper\"]\n    d --> claimed\n    a --> open[\"untouched, still open\"]\n    b --> open";

    #[test]
    fn unknown_mermaid_source_degrades_to_a_pre_block() {
        let out = render_mermaid("this diagram was never committed");
        assert!(
            out.contains("<pre class=\"mermaid\">"),
            "expected a visible fallback, got: {out}"
        );
        assert!(
            !out.contains("<figure"),
            "an unmatched diagram must not claim a figure: {out}"
        );
    }

    #[test]
    fn the_fallback_keeps_the_source_visible_and_escaped() {
        let out = render_mermaid("flowchart TD\n  A<B-->C");
        assert!(out.contains("A&lt;B"), "source lost or unescaped: {out}");
        assert!(!out.contains("A<B"), "raw markup leaked: {out}");
    }

    #[test]
    fn mermaid_resolves_to_a_committed_svg() {
        let out = render_mermaid(KNOWN);
        assert!(out.contains("<figure class=\"diagram\">"), "{out}");
        assert!(
            out.contains("/diagrams/feffermans-four-scenarios.svg"),
            "{out}"
        );
        assert!(!out.contains("<pre"), "fell back: {out}");
    }

    #[test]
    fn surrounding_whitespace_does_not_change_the_match() {
        let padded = format!("\n\n  {KNOWN}  \n\n");
        assert_eq!(render_mermaid(KNOWN), render_mermaid(&padded));
    }

    #[test]
    fn the_diagram_carries_its_size_and_title() {
        let out = render_mermaid(KNOWN);
        assert!(out.contains("alt=\""), "no alt text: {out}");
        assert!(out.contains(" width=\""), "no width: {out}");
        assert!(out.contains(" height=\""), "no height: {out}");
        assert!(out.contains("loading=\"lazy\""), "not lazy: {out}");
    }

    #[test]
    fn fnv1a_matches_its_published_test_vectors() {
        // The standard 64-bit FNV-1a vectors. A silent change to the hash
        // function would re-key every diagram at once, so it is pinned here.
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a("foobar"), 0x8594_4171_f739_67e8);
    }
}
