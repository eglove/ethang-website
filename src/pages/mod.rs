//! One module per page, each returning the HTML for that page.
//!
//! Filled in by Tasks 9 through 12.

use std::path::Path;

/// Render the whole site into `out`.
///
/// Still a stub: the binary wiring is in place so `build` is exercised end to
/// end, and the first real page arrives with Task 2.
pub fn build_site(out: &Path) -> anyhow::Result<()> {
    anyhow::bail!(
        "pages::build_site is not implemented yet (would write to {})",
        out.display()
    )
}