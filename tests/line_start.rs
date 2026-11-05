//! A run of digits and a `.` at the start of a line opens a numbered list
//! item in Typst markup. Prose that carries one there, after a soft break or
//! at the start of a paragraph, must stay prose.

use twig_export::{config::Config, render};

fn typst(source: &str) -> String {
    render(
        source,
        twig_markdown::format(),
        &Config::default(),
        twig_export::Extensions::default(),
    )
    .unwrap()
    .typst
}

const AFTER_A_SOFT_BREAK: &str = "It began in\n2024. We left.\n";
const AT_A_PARAGRAPH_START: &str = "Intro.\n\n2024\\. Was the year.\n";

#[test]
fn a_number_at_a_line_start_is_escaped() {
    assert!(typst(AFTER_A_SOFT_BREAK).contains("\n2024\\. We left."));
    assert!(typst(AT_A_PARAGRAPH_START).contains("2024\\. Was the year."));
}

/// What Typst itself reads: no list item, and a document that compiles.
#[cfg(feature = "pdf")]
#[test]
fn typst_reads_a_number_at_a_line_start_as_text() {
    use typst::syntax::{SyntaxKind, SyntaxNode};

    fn has_enum_item(node: &SyntaxNode) -> bool {
        node.kind() == SyntaxKind::EnumItem || node.children().any(has_enum_item)
    }

    for source in [AFTER_A_SOFT_BREAK, AT_A_PARAGRAPH_START] {
        let typst = typst(source);
        assert!(
            !has_enum_item(&typst::syntax::parse(&typst)),
            "a numbered list item in:\n{typst}"
        );
        twig_export::pdf::compile(&typst).expect("the generated Typst must compile");
    }
}
