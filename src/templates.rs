//! The templates twig-export ships: each the procedural half of a kind of
//! page, as Typst, a function `twig-template(body, layout:, paper:, width:,
//! height:, size:, face:, dark:)` that sets the page and styles every element
//! the emitter makes. A config names one (`document.template`); the emitter
//! writes its source above the document and applies it with `#show`.
//!
//! Each lays the same document out two ways: `layout: "paged"` on a sheet,
//! with its margins and page numbers; `layout: "reflow"` as one page `width`
//! wide and as tall as the text, for a document read on a screen. Sizes are
//! in em of `size`, so a reader's text size scales every style, and faces are
//! named by word, each with fallbacks (`docs/proposals` in leaf,
//! "Prestructured pages").

/// Every template, by name, and its Typst.
pub const TEMPLATES: &[(&str, &str)] = &[
    ("classic", include_str!("../templates/classic.typ")),
    ("modern", include_str!("../templates/modern.typ")),
    ("editorial", include_str!("../templates/editorial.typ")),
];

/// The names a config can give `document.template`.
pub const NAMES: &[&str] = &["classic", "modern", "editorial"];

/// The Typst of the template named `name`.
pub fn source(name: &str) -> Option<&'static str> {
    TEMPLATES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_has_a_template_that_defines_the_function() {
        assert_eq!(NAMES.len(), TEMPLATES.len());
        for name in NAMES {
            let src = source(name).expect("a template for every name");
            assert!(src.contains("#let twig-template("), "{name}");
        }
    }
}
