---
title: A tile keeps a frame only svg reads
description: '`Tile::frame` is read only by `tile_svg`, behind the `svg` feature, so a default build warns that it is never read and `cargo clippy --all-targets -- -D warnings` fails'
part_of: '[Tasks](/docs/tasks/tasks.md)'
status: open
created: 2026-10-28
---

# A tile keeps a frame only svg reads

469dafc gave `tiles::Tile` a `pub(crate) frame: Frame`, the tile's own
picture, and its one reader is `Typesetter::tile_svg` in `src/pdf.rs`, which
is `#[cfg(feature = "svg")]`. `svg` is not a default feature, so with the
default features (`pdf`, `network`) nothing reads the field. Rust's
dead-code lint fires on it, and `-D warnings` turns that into an error.

## Repro

At 469dafc:

```console
$ cargo build
warning: field `frame` is never read
  --> src/tiles.rs:41:16
$ cargo clippy --all-targets -- -D warnings
error: field `frame` is never read
$ cargo clippy --all-targets --features svg -- -D warnings   # clean
```

`cargo test` passes either way. twig-export has no CI workflow yet, so
nothing ran the lint before the commit landed.

## What it needs

`Tile` should build without the warning under every feature set the crate
offers. The field could be gated on `svg` with the code that fills it
(`tiles` builds the frame for every tile regardless), or kept for a reader
that is not `svg`'s, such as a PNG tile, and marked as such. Which one
depends on whether a tile's frame is meant to outlive the `svg` feature.

## Done when

`cargo clippy --all-targets -- -D warnings` passes with the default
features and with `--features svg`.
