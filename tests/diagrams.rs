//! Every mermaid fence in the corpus must resolve to a committed diagram.
//!
//! The failure mode this guards against is silent: a fence whose hash is not in
//! `DIAGRAMS` renders as a `<pre>` of its own source, which is valid HTML and
//! passes every other test in the crate. Counting the fences and requiring a
//! `<figure>` for each one is what turns that into a failure.

use std::path::{Path, PathBuf};

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The mermaid fences in `content/blog`, or - before Task 8 migrates the
/// corpus - the recorded sources in `tests/fixtures/mermaid`, one per fence.
fn mermaid_sources() -> Vec<String> {
    let content = manifest().join("content").join("blog");
    if content.is_dir() {
        return fences_in_corpus(&content);
    }
    fixtures()
}

fn fixtures() -> Vec<String> {
    let dir = manifest().join("tests").join("fixtures").join("mermaid");
    let mut sources: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("mmd"))
        .map(|path| {
            let body = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            body.trim_end_matches('\n').to_string()
        })
        .collect();
    sources.sort();
    sources
}

fn fences_in_corpus(root: &Path) -> Vec<String> {
    let mut sources = Vec::new();
    let mut posts: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap_or_else(|error| panic!("{}: {error}", root.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("index.mdx"))
        .filter(|path| path.is_file())
        .collect();
    posts.sort();
    for post in posts {
        let body = std::fs::read_to_string(&post)
            .unwrap_or_else(|error| panic!("{}: {error}", post.display()));
        let mut in_fence = false;
        let mut is_mermaid = false;
        let mut current: Vec<&str> = Vec::new();
        for line in body.lines() {
            if line.trim_start().starts_with("```") {
                if in_fence {
                    if is_mermaid {
                        sources.push(current.join("\n").trim().to_string());
                    }
                    in_fence = false;
                    is_mermaid = false;
                    current.clear();
                } else {
                    in_fence = true;
                    is_mermaid = line.trim_start().starts_with("```mermaid");
                }
                continue;
            }
            if in_fence && is_mermaid {
                current.push(line);
            }
        }
    }
    sources
}

#[test]
fn the_corpus_has_thirteen_mermaid_fences() {
    let sources = mermaid_sources();
    assert_eq!(
        sources.len(),
        13,
        "expected 13 mermaid fences, found {}: {sources:#?}",
        sources.len()
    );
}

#[test]
fn every_corpus_fence_matches_a_committed_diagram() {
    let mut unmatched = Vec::new();
    for source in mermaid_sources() {
        let out = ethang_website::diagrams::render_mermaid(&source);
        if !out.contains("<figure") {
            let head: String = source.lines().take(3).collect::<Vec<_>>().join(" / ");
            unmatched.push(head);
        }
    }
    assert!(
        unmatched.is_empty(),
        "{} mermaid fence(s) fell back to their source instead of a diagram. \
         Re-render them and update `DIAGRAMS` in src/diagrams.rs: {unmatched:#?}",
        unmatched.len()
    );
}

#[test]
fn no_diagram_is_committed_without_a_fence() {
    // The reverse drift: a diagram whose source was edited away would sit in
    // `static/diagrams` forever, never referenced and never noticed.
    let dir = manifest().join("static").join("diagrams");
    let committed = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("svg"))
        .count();
    assert_eq!(
        committed,
        ethang_website::diagrams::committed_diagrams(),
        "`static/diagrams` and `DIAGRAMS` disagree on how many diagrams exist"
    );
}

#[test]
fn every_committed_diagram_file_exists() {
    let dir = manifest().join("static").join("diagrams");
    for source in mermaid_sources() {
        let out = ethang_website::diagrams::render_mermaid(&source);
        let Some(at) = out.find("/diagrams/") else {
            continue;
        };
        let name: String = out[at + "/diagrams/".len()..]
            .chars()
            .take_while(|ch| *ch != '\u{22}')
            .collect();
        assert!(
            dir.join(&name).is_file(),
            "{name} is referenced but not committed under static/diagrams"
        );
    }
}

#[test]
fn committed_diagrams_are_self_contained_svg() {
    // No script, no external reference, no event handler: a diagram is an
    // image, and an image must not be able to run anything.
    let dir = manifest().join("static").join("diagrams");
    for entry in std::fs::read_dir(&dir).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("svg") {
            continue;
        }
        let svg = std::fs::read_to_string(&path).unwrap();
        assert!(svg.contains("<svg"), "{} is not an SVG", path.display());
        assert!(
            !svg.contains("<script"),
            "{} carries a script",
            path.display()
        );
        assert!(
            !svg.contains("javascript:"),
            "{} carries a javascript: URL",
            path.display()
        );
        for handler in ["onclick", "onload", "onerror", "onmouseover"] {
            assert!(
                !svg.contains(handler),
                "{} carries an {handler} handler",
                path.display()
            );
        }
    }
}
