//! LaTeX math rendered to MathML at build time.
//!
//! The posts are written in TeX; the site ships MathML. `latex2mathml` does the
//! conversion once, during the build, so a page needs no KaTeX script, no KaTeX
//! stylesheet, and none of the twenty font files the old site served: MathML is
//! native to every current browser.
//!
//! A malformed expression never fails the build. It degrades to the escaped
//! source in a `<code>`, with a warning naming the expression on stderr, so a
//! typo shows up in the output and in the log rather than taking the build down.

use crate::html::{el, escape, raw};
use latex2mathml::{DisplayStyle, latex_to_mathml};

/// Render one TeX span as MathML, wrapped in the element its display mode wants.
///
/// Inline math becomes `<span class="math">` and display math becomes
/// `<div class="math math--display">`: a display equation is a block, and the
/// stylesheet centres it on a line of its own.
///
/// This is the function `RenderCtx::render_math` is bound to, so it must return
/// the complete element, not a fragment to be wrapped later.
pub fn render_math(tex: &str, display: bool) -> String {
    let (tag, class) = if display {
        ("div", "math math--display")
    } else {
        ("span", "math")
    };
    let wrapper = |body: String| el(tag, &[("class", class)], &[body]);

    let trimmed = tex.trim();
    if trimmed.is_empty() {
        return wrapper(fallback(tex));
    }

    let style = if display {
        DisplayStyle::Block
    } else {
        DisplayStyle::Inline
    };
    // The parser is a hand-written recursive descent; a panic inside it would
    // take the whole build down over one bad equation, so it is caught here.
    match std::panic::catch_unwind(|| latex_to_mathml(trimmed, style)) {
        // Not every failure is an `Err`: an ampersand, for one, comes back as
        // `Ok` with a `[PARSE ERROR: ...]` marker embedded in the output. Left
        // alone that reaches the page as visible rubble, so it counts as a
        // failure too.
        Ok(Ok(mathml)) if mathml.contains("[PARSE ERROR") => {
            eprintln!("math: `{trimmed}` did not convert: {mathml}");
            wrapper(fallback(tex))
        }
        Ok(Ok(mathml)) => wrapper(raw(&mathml)),
        Ok(Err(error)) => {
            eprintln!("math: `{trimmed}` did not convert: {error}");
            wrapper(fallback(tex))
        }
        Err(_) => {
            eprintln!("math: `{trimmed}` panicked the LaTeX parser");
            wrapper(fallback(tex))
        }
    }
}

/// The visible stand-in for an expression that would not convert: the source
/// itself, escaped, so the reader sees what was written instead of a blank.
fn fallback(tex: &str) -> String {
    el("code", &[], &[escape(tex.trim())])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_inline_math_as_mathml() {
        let out = render_math("u(x, t)", false);
        assert!(out.contains("<math"));
        assert!(out.contains("class=\"math\""));
    }

    #[test]
    fn renders_display_math_in_a_block() {
        let out = render_math("\\nabla \\cdot u = 0", true);
        assert!(out.contains("math--display"));
    }

    #[test]
    fn handles_a_frac_expression() {
        let out = render_math("\\frac{\\partial u}{\\partial t}", false);
        assert!(out.contains("<math"), "latex2mathml failed: {out}");
    }

    #[test]
    fn invalid_latex_falls_back_to_the_escaped_source() {
        let out = render_math("\\frac{", false);
        assert!(
            out.contains("<code"),
            "expected a code fallback, got: {out}"
        );
        assert!(
            !out.contains("<math"),
            "invalid latex must not emit math: {out}"
        );
    }

    #[test]
    fn an_expression_the_parser_marks_as_broken_falls_back() {
        // `latex2mathml` answers `Ok` here, with the marker inlined in the
        // MathML. Rendering that would put `[PARSE ERROR: Ampersand]` on the
        // page, so the marker has to be treated as a failure.
        let out = render_math("x & y", false);
        assert!(
            out.contains("<code"),
            "expected a code fallback, got: {out}"
        );
        assert!(!out.contains("<math"), "broken math emitted: {out}");
        assert!(!out.contains("PARSE ERROR"), "parser marker leaked: {out}");
    }

    #[test]
    fn the_fallback_shows_the_source_escaped() {
        let out = render_math("\\frac{ a & b }", false);
        assert!(out.contains("&amp;"), "source not escaped: {out}");
        assert!(!out.contains("<math"), "broken math emitted: {out}");
    }

    #[test]
    fn empty_math_does_not_reach_the_parser() {
        let out = render_math("   ", false);
        assert!(out.contains("<code"), "{out}");
    }

    #[test]
    fn inline_math_is_a_span_and_display_math_is_a_div() {
        assert!(render_math("x", false).starts_with("<span class=\"math\">"));
        assert!(render_math("x", true).starts_with("<div class=\"math math--display\">"));
    }

    #[test]
    fn the_navier_stokes_equations_convert() {
        // The four display equations the corpus actually carries, all in
        // `the-blowup-half`. A regression here is a regression on real content.
        let equations = [
            r"\frac{\partial u}{\partial t} + (u \cdot \nabla) u = -\nabla p + \nu \nabla^2 u + f",
            r"\nabla \cdot u = 0",
            r"Re = UL/\nu",
            r"\tau^{1/2-h}",
        ];
        for equation in equations {
            let out = render_math(equation, true);
            assert!(out.contains("<math"), "{equation} failed: {out}");
            assert!(!out.contains("<code"), "{equation} fell back: {out}");
        }
    }
}
