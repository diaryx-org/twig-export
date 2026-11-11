---
title: A world keeps what only pdf reads
part_of: '[Tasks](/docs/tasks/tasks.md)'
status: done
created: 2026-11-09
updated: 2026-11-10
description: '`Packages::new`, `Packages::dirs` and four `ExportWorld` builders in `src/typst_world.rs` are used only by the `pdf` path, so a build with `html` and without `pdf` warns that they are never used and `cargo clippy --all-targets --no-default-features --features html -- -D warnings` fails'
---

# A world keeps what only pdf reads

`src/typst_world.rs` is compiled when either `pdf` or `html` is on
(`#[cfg(any(feature = "pdf", feature = "html"))]` in `src/lib.rs`), and
holds the `World` both backends compile through. Six of its items have
readers only behind `pdf`:

- `Packages::new` and `Packages::dirs`, read by `pdf::Typesetter` and by
  `src/main.rs`'s PDF path; `Packages` reaches callers only through `pdf`'s
  re-export.
- `ExportWorld::with_packages`, `with_today`, `with_files` and
  `main_source`, read by `pdf::Typesetter` (`src/pdf.rs`).

`html::compile` builds its world with `ExportWorld::new` alone. With `html`
and without `pdf`, nothing reads the six, Rust's dead-code lint fires, and
`-D warnings` makes it an error.

## Repro

At eb5f284:

```console
$ cargo build --no-default-features --features html
warning: associated items `new` and `dirs` are never used
  --> src/typst_world.rs:45:12
warning: methods `with_packages`, `with_today`, `with_files`, and `main_source` are never used
   --> src/typst_world.rs:173:19
$ cargo clippy --all-targets --no-default-features --features html -- -D warnings
error: associated items `new` and `dirs` are never used
error: methods `with_packages`, `with_today`, `with_files`, and `main_source` are never used
error: could not compile `twig-export` (lib) due to 2 previous errors
```

`cargo test --no-default-features --features html` passes. Clippy is clean
with the default features, `--no-default-features`, `--features svg` and
`--all-features`, so only the `html`-without-`pdf` build is affected.
twig-export has no CI workflow, so no lane builds that combination.

## What it needs

The crate should build without the warnings under every feature set it
offers. Either gate the six items on `pdf`, as 8c6b521 did for
`Tile::frame` under `svg`, or give the HTML backend a reader for them: as
written, `html::compile` takes no package folders, no `today` and no
host-found files, so `-o out.html` ignores the `--packages` folders a PDF
export would search. Which one depends on whether the HTML backend is meant
to resolve packages and pictures as the PDF backend does.

## Done when

`cargo clippy --all-targets -- -D warnings` passes with
`--no-default-features --features html`, as well as with the default
features, `--no-default-features`, `--features svg` and `--all-features`.

## Resolved

The six items are gated on `pdf`, as 8c6b521 gated `Tile::frame` on
`svg`. Nothing changes for a build with `pdf`; one with `html` alone
compiles them out. `Packages` reaches callers only through `pdf`'s
re-export, so no public item goes away. `html::compile` still takes no
package folders, no `today` and no host-found files; a backend that
should, takes the `cfg` off the ones it reads.
`cargo clippy --all-targets -- -D warnings` passes with the default
features, `--no-default-features`, `--no-default-features --features html`,
`--features html`, `--features svg` and `--all-features`.
