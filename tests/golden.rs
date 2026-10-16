//! Golden test over the whole AST→Typst mapping.
//!
//! This is why `.typ` is the real target rather than an implementation detail:
//! the entire interesting surface of this crate is a text-to-text function, so
//! one readable fixture pins every rendering decision at once and a regression
//! shows up as a diff you can read — not as a changed byte in a PDF.
//!
//! Regenerate after an intentional change:
//!
//! ```sh
//! UPDATE_GOLDEN=1 cargo test --no-default-features --test golden
//! ```

use std::path::Path;

use twig_export::{config::Config, render};

#[test]
fn report_example_renders_to_the_golden_typst() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("examples/report.md")).unwrap();
    let (cfg, _) = Config::load(&root.join("examples/report.toml")).unwrap();

    let rendered = render(
        &source,
        twig_markdown::format(),
        &cfg,
        twig_export::Extensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    // Every rule in the example config must match something. A config where a
    // rule silently matches nothing is the most common way styling "does not
    // work", so the example must not model that.
    for (rule, n) in cfg.rules.iter().zip(&rendered.match_counts) {
        assert!(*n > 0, "example rule matched nothing: {}", rule.select);
    }

    let golden_path = root.join("tests/golden/report.typ");
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, &rendered.typst).unwrap();
        return;
    }

    let expected = std::fs::read_to_string(&golden_path)
        .expect("missing golden file; regenerate with UPDATE_GOLDEN=1");
    assert_eq!(
        rendered.typst, expected,
        "generated Typst differs from tests/golden/report.typ; \
         regenerate with UPDATE_GOLDEN=1 if the change is intended"
    );
}

/// The `hidden` rule must remove content from the OUTPUT, not merely skip its
/// styling. This is the audience-filtering guarantee, so it gets its own
/// assertion rather than riding on the golden diff.
#[test]
fn hidden_directives_leave_no_trace_in_the_output() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(root.join("examples/report.md")).unwrap();
    let (cfg, _) = Config::load(&root.join("examples/report.toml")).unwrap();

    assert!(
        source.contains("dropped entirely"),
        "the fixture must contain the text that gets hidden"
    );

    let rendered = render(
        &source,
        twig_markdown::format(),
        &cfg,
        twig_export::Extensions {
            directives: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert!(
        !rendered.typst.contains("dropped entirely"),
        "a `hidden` rule must remove the text, not just its styling"
    );
}
