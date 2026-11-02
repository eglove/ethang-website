//! The migration is content, so its tests count content: 37 published posts
//! out of 39 directories, an images/ directory beside every post, the ten
//! Learn stages, and fonts that are served from this repo rather than a CDN.

use std::path::Path;

fn blog_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("content/blog")
}

#[test]
fn every_published_post_was_migrated() {
    let count = std::fs::read_dir(blog_root())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().join("index.mdx").exists())
        .count();
    assert_eq!(
        count, 37,
        "expected 37 published posts (39 directories, 2 drafts)"
    );
}

#[test]
fn drafts_were_not_migrated() {
    assert!(!blog_root().join("the-harness-is-the-orchestrator").exists());
    assert!(!blog_root().join("three-channels-for-reasoning").exists());
}

#[test]
fn every_post_has_an_images_directory() {
    for entry in std::fs::read_dir(blog_root())
        .unwrap()
        .filter_map(Result::ok)
    {
        let dir = entry.path();
        if !dir.join("index.mdx").exists() {
            continue;
        }
        assert!(
            dir.join("images").is_dir(),
            "missing images/ in {}",
            dir.display()
        );
    }
}

#[test]
fn learn_curriculum_has_ten_stages() {
    let text =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("content/learn.toml"))
            .unwrap();
    assert_eq!(text.matches("[[stages]]").count(), 10);
}

#[test]
fn fonts_were_self_hosted() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("static/fonts");
    let count = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .count();
    assert!(count >= 5, "expected at least 5 font files, found {count}");
}
