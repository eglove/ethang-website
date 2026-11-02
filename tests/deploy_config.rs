use std::path::Path;

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn config() -> String {
    let path = root().join("wrangler.jsonc");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn config_points_at_the_dist_directory() {
    assert!(config().contains("\"./dist\""));
}

#[test]
fn config_has_no_worker_entrypoint() {
    let config = config();
    assert!(
        !config.contains("\"main\""),
        "a worker main would make this a server, not a static site"
    );
    assert!(
        !config.contains("\"binding\""),
        "an assets binding only exists for a worker that reads the assets"
    );
}

#[test]
fn config_serves_the_404_page() {
    assert!(config().contains("404-page"));
}

#[test]
fn config_resolves_directory_style_routes() {
    // Every route is emitted as <route>/index.html, so the host has to answer
    // a slash-free request such as /blog/alpha from blog/alpha/index.html.
    assert!(config().contains("auto-trailing-slash"));
}

#[test]
fn no_typescript_config_exists() {
    for name in [
        "cloudflare.config.ts",
        "wrangler.config.ts",
        "vite.config.ts",
    ] {
        assert!(
            !root().join(name).exists(),
            "{name} is TypeScript; deployment must be configured with JSON"
        );
    }
}
