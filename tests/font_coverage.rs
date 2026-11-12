//! The web fonts are subsets, so every character the site renders has to be
//! one they carry.
//!
//! `quality/subset_fonts.py` cuts the faces down to the characters found in
//! the content and the page source, which halves every file and is most of
//! what keeps Largest Contentful Paint inside its budget. The trade is that a
//! character added later - a curly quote, an arrow, an accented name - would
//! render as a missing glyph with nothing to say so. This test is the thing
//! that says so.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every codepoint the subset fonts declare they carry.
fn covered_codepoints() -> BTreeSet<u32> {
    let path = root().join("static/fonts/covered-codepoints.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with("U+"))
        .filter_map(|line| u32::from_str_radix(&line[2..], 16).ok())
        .collect()
}

fn collect(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == "target" || name == ".git" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            collect(&path, extensions, out);
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| extensions.contains(&ext))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

/// Characters the site renders: the content, the page copy, and the stylesheet.
fn rendered_characters() -> BTreeSet<u32> {
    let mut files = Vec::new();
    for dir in ["content", "src", "static"] {
        collect(&root().join(dir), &["mdx", "md", "toml", "rs", "css", "svg"], &mut files);
    }
    let mut chars = BTreeSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for ch in text.chars() {
            // Control characters are not glyphs.
            if ch >= '\u{20}' {
                chars.insert(ch as u32);
            }
        }
    }
    chars
}

#[test]
fn every_rendered_character_is_carried_by_the_subset_fonts() {
    let covered = covered_codepoints();
    assert!(
        !covered.is_empty(),
        "static/fonts/covered-codepoints.txt is empty or missing; run: python quality/subset_fonts.py"
    );

    let missing: Vec<String> = rendered_characters()
        .into_iter()
        .filter(|code| !covered.contains(code))
        .map(|code| {
            let ch = char::from_u32(code).unwrap_or('?');
            format!("U+{code:04X} {ch:?}")
        })
        .collect();

    assert!(
        missing.is_empty(),
        "the subset fonts do not carry these characters: {missing:#?}\nrun: python quality/subset_fonts.py"
    );
}

#[test]
fn the_subset_fonts_are_smaller_than_the_originals() {
    // The point of subsetting is the byte count; a regenerated font that came
    // out the same size as its source means the subset step did nothing.
    let originals = root().join("assets/fonts");
    let published = root().join("static/fonts");
    let Ok(entries) = std::fs::read_dir(&originals) else {
        panic!("no assets/fonts directory");
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let original = entry.path();
        let subset = published.join(&name);
        assert!(
            subset.exists(),
            "{} was never subset; run: python quality/subset_fonts.py",
            name.to_string_lossy()
        );
        let original_size = std::fs::metadata(&original).unwrap().len();
        let subset_size = std::fs::metadata(&subset).unwrap().len();
        assert!(
            subset_size < original_size,
            "{} is {} bytes, not smaller than its {} byte source",
            name.to_string_lossy(),
            subset_size,
            original_size
        );
    }
}
