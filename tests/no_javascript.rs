use std::path::{Path, PathBuf};

const FORBIDDEN: [&str; 8] = ["js", "mjs", "cjs", "jsx", "ts", "mts", "cts", "tsx"];

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name == ".git" || name == "target" {
            continue;
        }
        if path.is_dir() {
            walk(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn offenders_in(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk(root, &mut files);
    files
        .iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .map(|e| FORBIDDEN.contains(&e))
                .unwrap_or(false)
        })
        .map(|p| p.display().to_string())
        .collect()
}

#[test]
fn repository_contains_no_javascript_or_typescript() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let offenders = offenders_in(root);
    assert!(offenders.is_empty(), "JavaScript/TypeScript files present: {offenders:#?}");
}

#[test]
fn build_output_contains_no_javascript_or_typescript() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("dist");
    if !dist.exists() {
        return;
    }
    let offenders = offenders_in(&dist);
    assert!(offenders.is_empty(), "build emitted JavaScript/TypeScript: {offenders:#?}");
}