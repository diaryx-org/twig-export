//! Tiles: a laid-out page cut into pieces to draw one by one, a piece for
//! each top-level block of the document as it is set on the page, with
//! whatever is drawn with it (a rule, a table's lines, a code block's ground).
//!
//! An editor that draws the pages as pictures and draws them again on every
//! keystroke draws the whole document every time: a long one is a picture of
//! megabytes, decoded and painted again for one letter. Cut into blocks, an
//! edit changes the picture of the block it is in; the blocks below it move,
//! and a block that moves draws the same. Each tile has a [`Tile::key`], a
//! hash of what it draws relative to its own top, so a host keeps the
//! pictures it has and makes only those it has not, and draws only the tiles
//! on screen.
//!
//! Which block a thing on the page belongs to is read from its glyphs'
//! spans (`pdf.rs`): a span's start is the block's index. A group whose
//! glyphs are all one block's is that block's whole; a group of several
//! blocks' (the page's body) is cut into its items. What carries no glyph (a
//! rule, a cell's border, a page number) goes with the block it lies beside.

use std::hash::{Hash, Hasher};

use typst::layout::{Abs, Frame, FrameItem, Point, Size, Transform};
use typst::syntax::{FileId, SpanKind};
use typst_layout::PagedDocument;

/// A piece of a page, drawn by itself.
#[derive(Clone)]
pub struct Tile {
    /// The page it is on.
    pub page: usize,
    /// The top-level block it draws, if it draws one's glyphs.
    pub block: Option<usize>,
    /// Where it is on its page, in points from the page's top: it is the
    /// page's width, from `top` down `height`.
    pub top: f64,
    pub height: f64,
    pub width: f64,
    /// A hash of what it draws, relative to its top: two tiles with the same
    /// key draw the same picture.
    pub key: u128,
    pub(crate) frame: Frame,
}

/// One thing on a page, where it is, how far down it reaches, and the block
/// it is drawn for.
struct Unit {
    pos: Point,
    item: FrameItem,
    top: Abs,
    bottom: Abs,
    block: Option<usize>,
}

/// The tiles of every page of `document`, whose glyphs' spans into `file`
/// name their blocks.
pub(crate) fn tiles(document: &PagedDocument, file: FileId) -> Vec<Tile> {
    let mut out = Vec::new();
    for (i, page) in document.pages().iter().enumerate() {
        let frame = &page.frame;
        let mut units = Vec::new();
        cut(frame, Point::zero(), file, &mut units);
        assign(&mut units);
        // A tile for each block, in the order the page draws them.
        let mut order: Vec<Option<usize>> = Vec::new();
        for u in &units {
            if !order.contains(&u.block) {
                order.push(u.block);
            }
        }
        let width = frame.width();
        for block in order {
            let mine: Vec<&Unit> = units.iter().filter(|u| u.block == block).collect();
            let top = mine.iter().map(|u| u.top).fold(Abs::inf(), Abs::min);
            let bottom = mine.iter().map(|u| u.bottom).fold(-Abs::inf(), Abs::max);
            let top = top.max(Abs::zero()).min(frame.height());
            let bottom = bottom.min(frame.height()).max(top);
            let height = (bottom - top).max(Abs::pt(1.0));
            let mut tile = Frame::soft(Size::new(width, height));
            for u in &mine {
                tile.push(Point::new(u.pos.x, u.pos.y - top), u.item.clone());
            }
            let key = typst::utils::hash128(&(Drawn(&tile), width.to_raw().to_bits()));
            out.push(Tile {
                page: i,
                block,
                top: top.to_pt(),
                height: height.to_pt(),
                width: width.to_pt(),
                key,
                frame: tile,
            });
        }
    }
    out
}

/// `frame`'s items at `offset`, as units: a group of one block's glyphs, or
/// of none, whole; a group of several blocks', its items.
fn cut(frame: &Frame, offset: Point, file: FileId, out: &mut Vec<Unit>) {
    for (pos, item) in frame.items() {
        let at = offset + *pos;
        match item {
            FrameItem::Tag(_) => {}
            FrameItem::Group(g)
                if is_translation(g.transform)
                    && g.clip.is_none()
                    && blocks_in(&g.frame, file, &mut Vec::new()) > 1 =>
            {
                cut(
                    &g.frame,
                    at + Point::new(g.transform.tx, g.transform.ty),
                    file,
                    out,
                );
            }
            _ => {
                let (top, bottom) = extent(item, at);
                out.push(Unit {
                    pos: at,
                    item: item.clone(),
                    top,
                    bottom,
                    block: block_of(item, file),
                });
            }
        }
    }
}

fn is_translation(t: Transform) -> bool {
    t.sx.get() == 1.0 && t.sy.get() == 1.0 && t.kx.get() == 0.0 && t.ky.get() == 0.0
}

/// How many blocks' glyphs `frame` holds, up to two, gathered in `seen`.
fn blocks_in(frame: &Frame, file: FileId, seen: &mut Vec<usize>) -> usize {
    for (_, item) in frame.items() {
        if seen.len() > 1 {
            break;
        }
        match item {
            FrameItem::Group(g) => {
                blocks_in(&g.frame, file, seen);
            }
            FrameItem::Text(text) => {
                for g in &text.glyphs {
                    if let Some(b) = block(g.span.0, file)
                        && !seen.contains(&b)
                    {
                        seen.push(b);
                    }
                }
            }
            _ => {}
        }
    }
    seen.len()
}

/// The block whose glyphs an item draws: the first glyph's with a span into
/// the document.
fn block_of(item: &FrameItem, file: FileId) -> Option<usize> {
    match item {
        FrameItem::Text(text) => text.glyphs.iter().find_map(|g| block(g.span.0, file)),
        FrameItem::Group(g) => g.frame.items().find_map(|(_, i)| block_of(i, file)),
        _ => None,
    }
}

fn block(span: typst::syntax::Span, file: FileId) -> Option<usize> {
    match span.get() {
        SpanKind::Range { id, range } if id == file => Some(range.start),
        _ => None,
    }
}

/// How far down an item at `at` reaches: its top and its bottom.
fn extent(item: &FrameItem, at: Point) -> (Abs, Abs) {
    match item {
        FrameItem::Text(text) => {
            let m = text.font.metrics();
            (
                at.y - m.ascender.at(text.size),
                at.y - m.descender.at(text.size),
            )
        }
        FrameItem::Group(g) => {
            let size = g.frame.size();
            let corners = [
                Point::zero(),
                Point::new(size.x, Abs::zero()),
                Point::new(Abs::zero(), size.y),
                Point::new(size.x, size.y),
            ];
            let ys = corners.map(|c| (at + c.transform(g.transform)).y);
            let mut top = ys.iter().copied().fold(Abs::inf(), Abs::min);
            let mut bottom = ys.iter().copied().fold(-Abs::inf(), Abs::max);
            // What is drawn past the frame's edge, unclipped, is drawn too: an
            // underline under a paragraph's last line, below its frame.
            if g.clip.is_none() && is_translation(g.transform) {
                let origin = at + Point::new(g.transform.tx, g.transform.ty);
                for (pos, item) in g.frame.items() {
                    let (t, b) = extent(item, origin + *pos);
                    top = top.min(t);
                    bottom = bottom.max(b);
                }
            }
            (top, bottom)
        }
        FrameItem::Shape(shape, _) => {
            let b = shape.bbox(true);
            (at.y + b.min.y, at.y + b.max.y)
        }
        FrameItem::Image(_, size, _) | FrameItem::Link(_, size) => (at.y, at.y + size.y),
        FrameItem::Tag(_) => (at.y, at.y),
    }
}

/// Give each unit drawn for no block the block it lies beside: the one whose
/// glyphs reach over its middle, or the nearest.
fn assign(units: &mut [Unit]) {
    let mut spans: Vec<(usize, Abs, Abs)> = Vec::new();
    for u in units.iter() {
        let Some(b) = u.block else { continue };
        match spans.iter_mut().find(|s| s.0 == b) {
            Some(s) => {
                s.1 = s.1.min(u.top);
                s.2 = s.2.max(u.bottom);
            }
            None => spans.push((b, u.top, u.bottom)),
        }
    }
    if spans.is_empty() {
        return;
    }
    for u in units.iter_mut().filter(|u| u.block.is_none()) {
        let mid = (u.top + u.bottom) / 2.0;
        let distance = |s: &(usize, Abs, Abs)| {
            if mid < s.1 {
                s.1 - mid
            } else if mid > s.2 {
                mid - s.2
            } else {
                Abs::zero()
            }
        };
        u.block = spans
            .iter()
            .min_by(|a, b| distance(a).cmp(&distance(b)))
            .map(|s| s.0);
    }
}

/// A frame hashed by what it draws: its glyphs' spans, which count from the
/// block's place in the document, are left out, so a block that moves keeps
/// its key.
struct Drawn<'a>(&'a Frame);

impl Hash for Drawn<'_> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.0.size().hash(h);
        for (pos, item) in self.0.items() {
            pos.hash(h);
            match item {
                FrameItem::Group(g) => {
                    0u8.hash(h);
                    g.transform.hash(h);
                    g.clip.hash(h);
                    Drawn(&g.frame).hash(h);
                }
                FrameItem::Text(t) => {
                    1u8.hash(h);
                    t.font.hash(h);
                    t.size.hash(h);
                    t.fill.hash(h);
                    t.stroke.hash(h);
                    t.text.hash(h);
                    for g in &t.glyphs {
                        (
                            g.id,
                            g.x_advance,
                            g.x_offset,
                            g.y_advance,
                            g.y_offset,
                            &g.range,
                        )
                            .hash(h);
                    }
                }
                FrameItem::Shape(shape, _) => {
                    2u8.hash(h);
                    shape.hash(h);
                }
                FrameItem::Image(image, size, _) => {
                    3u8.hash(h);
                    image.hash(h);
                    size.hash(h);
                }
                FrameItem::Link(dest, size) => {
                    4u8.hash(h);
                    dest.hash(h);
                    size.hash(h);
                }
                FrameItem::Tag(_) => {}
            }
        }
    }
}
