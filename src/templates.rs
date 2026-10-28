//! Templates: each the procedural half of a kind of page, as a Typst package
//! whose entrypoint defines a function `twig-template(body, layout:, paper:,
//! width:, height:, size:, face:, dark:, meta:)` that sets the page and
//! styles every element the emitter makes. A config names one
//! (`document.template`); the emitter imports it and applies it with `#show`.
//!
//! Each lays the same document out in the layouts its manifest lists:
//! `layout: "paged"` on a sheet, with its margins and page numbers;
//! `layout: "reflow"` as one page `width` wide and as tall as the text, for a
//! document read on a screen. Sizes are in em of `size`, so a reader's text
//! size scales every style, and faces are named by word, each with fallbacks
//! (`docs/proposals` in leaf, "Prestructured pages").
//!
//! ── Why a package ──────────────────────────────────────────────────────────
//! A template is code someone else wrote, run over the reader's document.
//! Typst confines a package to its own files: a path a package names
//! resolves inside the package, so a template cannot read the document's
//! folder, or anything beside it, and set it into the page. twig-export
//! ships three, served from memory under the `twig` namespace
//! (`@twig/classic:0.1.0`); any other is a package on disk, laid out as
//! Typst lays out its own (`namespace/name/version/`), and named by its
//! spec. A package is a twig-export template when its `typst.toml` has a
//! `[tool.twig]` table: the name a menu shows, and the layouts it sets.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// The namespace the built-in templates are served under.
pub const NAMESPACE: &str = "twig";

/// The version every built-in template is served at.
pub const VERSION: &str = "0.1.0";

/// A template twig-export ships.
pub struct Builtin {
    pub name: &'static str,
    /// Its `typst.toml`.
    pub manifest: &'static str,
    /// Its entrypoint, `lib.typ`.
    pub source: &'static str,
}

macro_rules! builtin {
    ($name:literal) => {
        Builtin {
            name: $name,
            manifest: include_str!(concat!("../templates/", $name, "/typst.toml")),
            source: include_str!(concat!("../templates/", $name, "/lib.typ")),
        }
    };
}

/// Every template twig-export ships.
pub const BUILTIN: &[Builtin] = &[
    builtin!("classic"),
    builtin!("modern"),
    builtin!("editorial"),
];

/// The names a config can give `document.template` for a built-in.
pub const NAMES: &[&str] = &["classic", "modern", "editorial"];

/// The built-in template named `name`.
pub fn builtin(name: &str) -> Option<&'static Builtin> {
    BUILTIN.iter().find(|b| b.name == name)
}

/// The package a config's `document.template` names: a built-in by its
/// name, or any package by its spec (`@local/letter:1.0.0`). `None` for a
/// name that is neither.
pub fn spec(template: &str) -> Option<String> {
    if builtin(template).is_some() {
        return Some(format!("@{NAMESPACE}/{template}:{VERSION}"));
    }
    Spec::parse(template).map(|s| s.to_string())
}

/// A package spec, `@namespace/name:major.minor.patch`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Spec {
    pub namespace: String,
    pub name: String,
    pub version: (u32, u32, u32),
}

impl Spec {
    /// `s` read as a spec, if it is one: names as Typst allows them.
    pub fn parse(s: &str) -> Option<Spec> {
        let (namespace, rest) = s.strip_prefix('@')?.split_once('/')?;
        let (name, version) = rest.split_once(':')?;
        let ident = |s: &str| {
            s.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        };
        if !ident(namespace) || !ident(name) {
            return None;
        }
        let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
        let version = (parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then(|| Spec {
            namespace: namespace.into(),
            name: name.into(),
            version,
        })
    }

    /// Where the package lives under `dir`, laid out as Typst's own.
    pub fn dir_in(&self, dir: &Path) -> PathBuf {
        let (a, b, c) = self.version;
        dir.join(&self.namespace)
            .join(&self.name)
            .join(format!("{a}.{b}.{c}"))
    }
}

impl std::fmt::Display for Spec {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (a, b, c) = self.version;
        write!(f, "@{}/{}:{a}.{b}.{c}", self.namespace, self.name)
    }
}

/// What twig-export reads of a package's `typst.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    pub package: Package,
    /// Set when the package is a Typst Universe template: the starter
    /// project it copies, which shows how its function is called.
    #[serde(default)]
    pub template: Option<Starter>,
    #[serde(default)]
    pub tool: Tool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub entrypoint: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// The least Typst version the package compiles under.
    #[serde(default)]
    pub compiler: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Starter {
    pub path: String,
    pub entrypoint: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Tool {
    #[serde(default)]
    pub twig: Option<Twig>,
}

/// A package's `[tool.twig]`: what makes it a twig-export template.
#[derive(Clone, Debug, Deserialize)]
pub struct Twig {
    /// The name a menu shows.
    pub label: String,
    /// The layouts it sets, of "paged" and "reflow".
    pub layouts: Vec<String>,
}

impl Manifest {
    /// A `typst.toml` read.
    pub fn parse(toml: &str) -> Result<Manifest> {
        let format = crate::meta::fig_format("toml").context("fig reads no TOML")?;
        fig::from_slice(toml.as_bytes(), format)
            .map_err(|e| anyhow::anyhow!("{e}"))
            .context("reading typst.toml")
    }

    /// Its `[tool.twig]`, when it is a twig-export template.
    pub fn twig(&self) -> Option<&Twig> {
        self.tool.twig.as_ref()
    }

    /// Whether it sets `layout`.
    pub fn sets(&self, layout: &str) -> bool {
        self.twig()
            .is_some_and(|t| t.layouts.iter().any(|l| l == layout))
    }
}

impl Builtin {
    /// Its manifest, read.
    pub fn manifest(&self) -> Manifest {
        Manifest::parse(self.manifest).expect("a built-in template's manifest reads")
    }
}

/// A template on disk.
#[derive(Clone, Debug)]
pub struct Installed {
    pub spec: Spec,
    pub manifest: Manifest,
    pub dir: PathBuf,
}

/// Every twig-export template under `dir`, a folder of packages laid out as
/// Typst's own, in the order of their specs. A package that is not one, or
/// whose manifest does not read or does not name the place it is in, is
/// left out.
pub fn installed(dir: &Path) -> Vec<Installed> {
    let entries = |p: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect()
    };
    let name = |p: &Path| {
        p.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned()
    };
    let mut found = Vec::new();
    for ns in entries(dir) {
        for pkg in entries(&ns) {
            for ver in entries(&pkg) {
                let spec = format!("@{}/{}:{}", name(&ns), name(&pkg), name(&ver));
                let Some(spec) = Spec::parse(&spec) else {
                    continue;
                };
                let Ok(toml) = std::fs::read_to_string(ver.join("typst.toml")) else {
                    continue;
                };
                let Ok(manifest) = Manifest::parse(&toml) else {
                    continue;
                };
                if manifest.twig().is_none()
                    || manifest.package.name != spec.name
                    || Spec::parse(&format!("@x/x:{}", manifest.package.version))
                        .is_none_or(|v| v.version != spec.version)
                {
                    continue;
                }
                found.push(Installed {
                    spec,
                    manifest,
                    dir: ver,
                });
            }
        }
    }
    found.sort_by(|a, b| a.spec.cmp(&b.spec));
    found
}

/// The document a template is checked with (`pdf::Typesetter::check`):
/// the frontmatter most templates set from, and every construct the
/// emitter makes.
pub const SAMPLE: &str = "\
---
title: A sample document
subtitle: As a template sets it
author: Ada Lovelace
date: 2026-10-27
abstract: A short document with every construct, to set a template with.
keywords: [sample, template]
---

# A title

A paragraph with *emphasis*, **strong**, `code`, [a link](https://example.com),
and a footnote[^1]. It goes on long enough to wrap across a line or two, so
that the leading and the measure show.

## A section

> A quotation, set apart.

1. One
2. Two

- A bullet
- Another

### A subsection

| Name | Qty |
|------|----:|
| Pear | 3 |

```rust
fn main() {}
```

---

A last paragraph.

[^1]: The note.
";

/// `typ` with each built-in template's import replaced by its source: the
/// Typst written for typst-cli, which serves no `twig` namespace.
pub fn inline(typ: &str) -> String {
    let mut out = String::with_capacity(typ.len());
    for line in typ.split_inclusive('\n') {
        let template = line
            .strip_prefix(&format!("#import \"@{NAMESPACE}/"))
            .and_then(|rest| rest.split_once(':'))
            .and_then(|(name, _)| builtin(name));
        match template {
            Some(t) => {
                out.push_str(t.source);
                out.push('\n');
            }
            None => out.push_str(line),
        }
    }
    out
}

/// Check that a template named by a config is one there is: a built-in, or
/// a spec.
pub fn check(template: &str) -> Result<()> {
    if spec(template).is_none() {
        bail!(
            "no template named {template:?}: twig-export has {}, \
             or name a package by its spec (@namespace/name:1.0.0)",
            NAMES.join(", ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_is_a_template_package_at_the_version() {
        assert_eq!(NAMES.len(), BUILTIN.len());
        for name in NAMES {
            let b = builtin(name).expect("a template for every name");
            assert!(b.source.contains("#let twig-template("), "{name}");
            let m = b.manifest();
            assert_eq!(m.package.name, *name);
            assert_eq!(m.package.version, VERSION);
            assert_eq!(m.package.entrypoint, "lib.typ");
            assert!(m.sets("paged") && m.sets("reflow"), "{name}");
        }
    }

    #[test]
    fn a_template_is_a_builtin_or_a_spec() {
        assert_eq!(spec("classic").as_deref(), Some("@twig/classic:0.1.0"));
        assert_eq!(
            spec("@local/letter:1.2.3").as_deref(),
            Some("@local/letter:1.2.3")
        );
        for bad in [
            "fancy",
            "@local/letter",
            "@local/letter:1.2",
            "@../x:1.0.0",
            "@a/b:1.0.0.0",
        ] {
            assert_eq!(spec(bad), None, "{bad}");
        }
    }

    #[test]
    fn inlining_replaces_only_a_builtin_import() {
        let typ = "#import \"@twig/modern:0.1.0\": twig-template\n\
                   #import \"@preview/x:1.0.0\": y\nbody\n";
        let out = inline(typ);
        assert!(out.contains("#let twig-template("));
        assert!(out.contains("#import \"@preview/x:1.0.0\": y\nbody\n"));
        assert!(!out.contains("@twig/"));
    }
}
