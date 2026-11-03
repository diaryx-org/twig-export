//! What the Typst linked here leaves out (Cargo.toml turns off the fork's
//! default features) and what it keeps: a template that only names a part
//! left out still sets, and calling one fails with Typst's reason, not a
//! panic.

#![cfg(feature = "pdf")]

use twig_export::pdf::compile;

/// Why `typ` does not compile.
fn refused(typ: &str) -> String {
    format!("{:#}", compile(typ).expect_err("it should not compile"))
}

/// A template may style bibliographies and citations, as one written for a
/// whole Typst does, and name them as elements, as hydra does.
#[test]
fn a_template_that_styles_bibliographies_sets() {
    compile(
        "#set bibliography(style: \"apa\", title: none)\n\
         #show bibliography: set text(size: 9pt)\n\
         #show cite: emph\n\
         #let kinds = (bibliography, cite, heading)\n\
         #context query(selector(bibliography)).len()\n\
         Body.\n",
    )
    .expect("styling what is never shown sets");
}

#[test]
fn a_bibliography_is_refused() {
    let err = refused("#bibliography(bytes(\"@book{a, title={A}}\"))\n");
    assert!(err.contains("bibliographies are not supported"), "{err}");
}

#[test]
fn a_citation_is_refused() {
    // As in a document without a bibliography, which every one is here.
    let err = refused("#cite(<key>)\n");
    assert!(
        err.contains("the document does not contain a bibliography"),
        "{err}"
    );
}

#[test]
fn a_plugin_is_refused() {
    let err = refused("#plugin(bytes((0, 97, 115, 109)))\n");
    assert!(
        err.contains("WebAssembly plugins are not supported"),
        "{err}"
    );
}

#[test]
fn a_pdf_picture_is_refused() {
    let err = refused("#image(bytes(\"%PDF-1.7\\n\"), format: \"pdf\")\n");
    assert!(err.contains("PDF images are not supported"), "{err}");
}

#[test]
fn the_csv_yaml_and_cbor_loaders_are_gone() {
    for call in ["csv", "yaml", "cbor"] {
        let err = refused(&format!("#{call}(bytes(\"\"))\n"));
        assert!(
            err.contains(&format!("unknown variable: {call}")),
            "{call}: {err}"
        );
    }
}

/// Kept: `lorem` (examples/folklore-book.toml), code highlighting, the
/// loaders whose crates Typst needs anyway, and math's symbols.
#[test]
fn what_is_kept_still_sets() {
    compile(
        "#lorem(20)\n\n\
         ```rust\nfn main() { println!(\"hi\"); }\n```\n\n\
         #json(bytes(\"[1]\")).len() #toml(bytes(\"a = 1\")).a \
         #xml(bytes(\"<a/>\")).len()\n\n\
         #sym.arrow.r $arrow.r integral_0^1 x dif x$\n",
    )
    .expect("what is kept sets");
}
