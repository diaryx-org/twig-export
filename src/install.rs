//! Installing packages: a Typst Universe package, and every Universe
//! package it imports, fetched into a folder of packages, so that a world
//! that never reaches the network ([`crate::pdf::Packages`]) finds them
//! there.
//!
//! Installing is the one step that reaches the network, and it is asked
//! for: an editor installs a template when its reader picks one, then sets
//! documents offline, the same way every time. The caller gives the
//! [`Downloader`], so an editor fetches with the HTTP client it has.
//!
//! What is installed is checked before it is kept: an archive with a
//! symlink in it is refused, since a package's files are read as its own
//! and a symlink could make another file one; so is a package whose
//! manifest names another package, or a Typst newer than this one.

use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use typst::diag::PackageError;
use typst::syntax::package::{PackageSpec, PackageVersion, VersionBound};
use typst_kit::packages::{FsPackages, UniversePackages};

pub use typst_kit::downloader::Downloader;

use crate::templates::{Manifest, Spec};

/// The namespace Typst Universe serves.
pub const UNIVERSE: &str = "preview";

/// Fetch `spec` and every Typst Universe package it imports, however
/// deeply, into `dir`, laid out as Typst's own. A package already there is
/// kept as it is. Answers the packages fetched, in the order they were.
pub fn install(spec: &Spec, dir: &Path, downloader: impl Downloader) -> Result<Vec<Spec>> {
    let universe = UniversePackages::new(downloader);
    let store = FsPackages::new(dir);
    let mut fetched = Vec::new();
    let mut seen = Vec::new();
    let mut queue = vec![spec.clone()];
    while let Some(spec) = queue.pop() {
        if seen.contains(&spec) {
            continue;
        }
        seen.push(spec.clone());
        let at = spec.dir_in(dir);
        if !at.is_dir() {
            if spec.namespace != UNIVERSE {
                bail!("{spec} is not installed, and only {UNIVERSE} packages can be fetched");
            }
            let typst_spec: PackageSpec = spec
                .to_string()
                .parse()
                .map_err(|e| anyhow!("{spec}: {e}"))?;
            let mut archive = universe
                .package(&typst_spec)
                .map_err(|e| anyhow!("fetching {spec}: {e}"))?;
            store
                .store(&typst_spec, |tmp| {
                    archive
                        .unpack(tmp)
                        .map_err(|e| PackageError::MalformedArchive(Some(e.to_string().into())))?;
                    refuse_symlinks(tmp)
                        .map_err(|e| PackageError::MalformedArchive(Some(e.to_string().into())))
                })
                .map_err(|e| anyhow!("installing {spec}: {e}"))?;
            if let Err(e) = check(&spec, &at) {
                let _ = std::fs::remove_dir_all(&at);
                return Err(e);
            }
            fetched.push(spec.clone());
        }
        queue.extend(imports(&at)?.into_iter().filter(|s| *s != spec));
    }
    Ok(fetched)
}

/// The packages the Typst files under `dir` import.
pub fn imports(dir: &Path) -> Result<Vec<Spec>> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).with_context(|| format!("reading {}", d.display()))? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "typ") {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for spec in specs_in(&text) {
                    if !found.contains(&spec) {
                        found.push(spec);
                    }
                }
            }
        }
    }
    found.sort();
    Ok(found)
}

/// Every package spec a Typst source names in a string.
fn specs_in(text: &str) -> Vec<Spec> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter_map(Spec::parse)
        .collect()
}

/// That nothing under `dir` is a symlink.
fn refuse_symlinks(dir: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(std::io::Error::other(format!(
                "the archive has a symlink, {}",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            refuse_symlinks(&entry.path())?;
        }
    }
    Ok(())
}

/// That the package at `dir` is `spec`, for a Typst this one is.
fn check(spec: &Spec, dir: &Path) -> Result<()> {
    let toml = std::fs::read_to_string(dir.join("typst.toml"))
        .with_context(|| format!("{spec} has no typst.toml"))?;
    let manifest = Manifest::parse(&toml)?;
    if manifest.package.name != spec.name
        || Spec::parse(&format!("@x/x:{}", manifest.package.version))
            .is_none_or(|v| v.version != spec.version)
    {
        bail!("{spec}'s manifest names another package");
    }
    if let Some(required) = &manifest.package.compiler {
        let bound: VersionBound = required
            .parse()
            .map_err(|e| anyhow!("{spec} names a Typst it needs as {required:?}: {e}"))?;
        let current = PackageVersion::compiler();
        if !current.matches_ge(&bound) {
            bail!("{spec} needs Typst {required} or newer, and this is Typst {current}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_specs_a_source_imports_are_read_from_its_strings() {
        let src = "#import \"@preview/a:1.0.0\": x\n#let s = \"not a spec\"\n\
                   #import \"@preview/b:0.2.3\"\n#import \"lib.typ\"";
        let specs: Vec<String> = specs_in(src).iter().map(|s| s.to_string()).collect();
        assert_eq!(specs, ["@preview/a:1.0.0", "@preview/b:0.2.3"]);
    }
}
