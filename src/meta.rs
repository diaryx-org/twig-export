//! Frontmatter: the document's half of the configuration.
//!
//! Twig parses frontmatter into a `metadata` node — an inert data island of
//! `{lang, text}`, where `lang` is the config language the author's fence tag
//! named (`yaml` for a bare `---`, `toml` for `+++`, or an explicit tag) and
//! `text` is the raw bytes, unnormalised. Twig deliberately does not interpret
//! it (`AST-KINDS.md`: "a data island, not markup").
//!
//! Interpreting it is our job, and it is a one-liner because `metadata.lang`
//! names exactly the set of languages fig parses. That is not a coincidence
//! worth relying on silently, so: fig and twig are sister projects with the
//! same format vocabulary, which is why this module is small and why
//! [`crate::config`] reuses the same parser for the config file itself.
//!
//! ── The document proposes, the config disposes ─────────────────────────────
//! Nothing here reads a key on its own initiative. [`apply_metadata`] only
//! consults keys the config's `from-metadata` table explicitly bound, so a
//! document carrying `paper: a3` changes nothing unless the config said
//! `paper = "paper"`. Rendering an untrusted document cannot be made to do
//! something the config did not sanction.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde_json::Value as Json;

use crate::config::Document;

/// Map a language name — a `metadata.lang` fence tag, or a config file
/// extension — to a fig parser.
pub fn fig_format(lang: &str) -> Option<fig::Format> {
    Some(match lang.trim().to_ascii_lowercase().as_str() {
        "json" => fig::Format::Json,
        "jsonc" => fig::Format::Jsonc,
        "json5" => fig::Format::Json5,
        "yaml" | "yml" => fig::Format::Yaml,
        "toml" => fig::Format::Toml,
        "zon" => fig::Format::Zon,
        "fig" | "figl" => fig::Format::Fig,
        _ => return None,
    })
}

/// Parse a metadata island into a flat key/value map.
///
/// Returns an empty map for a language fig does not know, rather than an
/// error: an unrecognised frontmatter dialect should not stop a document from
/// rendering, it should just mean the document says nothing about its own
/// styling. A parse *failure* in a known language IS an error, because that is
/// a broken document the author wants to hear about.
pub fn parse_metadata(lang: &str, text: &str) -> Result<BTreeMap<String, Json>> {
    let Some(format) = fig_format(lang) else {
        return Ok(BTreeMap::new());
    };
    let value: Json = fig::from_slice(text.as_bytes(), format)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("parsing {lang} frontmatter"))?;

    Ok(match value {
        Json::Object(map) => map.into_iter().collect(),
        // A frontmatter block that is a scalar or a list has no keys to bind.
        _ => BTreeMap::new(),
    })
}

/// Overlay the metadata keys the config bound onto the document settings.
///
/// Returns the names of bindings whose key was absent from the document, which
/// the CLI reports under `--explain` — a binding that never fires is usually a
/// typo in one of the two files, and it is otherwise completely silent.
pub fn apply_metadata(doc: &mut Document, meta: &BTreeMap<String, Json>) -> Vec<String> {
    let bindings = doc.from_metadata.clone();
    let mut unfired = Vec::new();

    for (field, key) in &bindings {
        let Some(value) = meta.get(key) else {
            unfired.push(format!("{field} <- {key}"));
            continue;
        };
        let as_string = || match value {
            Json::String(s) => Some(s.clone()),
            Json::Number(n) => Some(n.to_string()),
            Json::Bool(b) => Some(b.to_string()),
            // A list-valued `author` is normal in frontmatter.
            Json::Array(items) => Some(
                items
                    .iter()
                    .filter_map(|i| match i {
                        Json::String(s) => Some(s.clone()),
                        Json::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            _ => None,
        };

        match field.as_str() {
            "paper" => doc.paper = as_string(),
            "margin" => doc.margin = as_string(),
            "font" => doc.font = as_string(),
            "size" => doc.size = as_string(),
            "leading" => doc.leading = as_string(),
            "lang" => doc.lang = as_string(),
            "title" => doc.title = as_string(),
            "author" => doc.author = as_string(),
            "page-numbering" | "page_numbering" => doc.page_numbering = as_string(),
            "toc-depth" | "toc_depth" => {
                doc.toc_depth = value.as_u64().map(|d| d as u8);
            }
            // An unknown target field is a config bug, but not one worth
            // aborting a render over; surface it the same way as an unfired
            // binding.
            other => unfired.push(format!("{other} <- {key} (no such document field)")),
        }
    }
    unfired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_bound_keys_take_effect() {
        let mut doc = Document {
            paper: Some("a4".into()),
            ..Default::default()
        };
        doc.from_metadata.insert("title".into(), "title".into());

        let meta: BTreeMap<String, Json> = [
            ("title".to_string(), Json::String("Real Title".into())),
            // Not bound by the config, so it must be inert.
            ("paper".to_string(), Json::String("a3".into())),
        ]
        .into_iter()
        .collect();

        apply_metadata(&mut doc, &meta);
        assert_eq!(doc.title.as_deref(), Some("Real Title"));
        assert_eq!(
            doc.paper.as_deref(),
            Some("a4"),
            "unbound key must be inert"
        );
    }

    #[test]
    fn a_binding_that_never_fires_is_reported() {
        let mut doc = Document::default();
        doc.from_metadata.insert("paper".into(), "pdf-paper".into());
        let unfired = apply_metadata(&mut doc, &BTreeMap::new());
        assert_eq!(unfired, vec!["paper <- pdf-paper"]);
    }

    #[test]
    fn list_valued_author_joins() {
        let mut doc = Document::default();
        doc.from_metadata.insert("author".into(), "author".into());
        let meta = [(
            "author".to_string(),
            Json::Array(vec![Json::String("A".into()), Json::String("B".into())]),
        )]
        .into_iter()
        .collect();
        apply_metadata(&mut doc, &meta);
        assert_eq!(doc.author.as_deref(), Some("A, B"));
    }
}
