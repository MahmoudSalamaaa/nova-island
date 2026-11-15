//! Ready layouts: rows of widgets, each row one wide widget or two small ones, all GAP apart, so
//! the block is always tidy. The first in a row is on the right (as Arabic reads).
use crate::widget::GAP;

/// A place in a layout: one wide widget (370 DIPs), or one small (176; a row holds two).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Slot {
    Wide,
    Small,
}
use Slot::*;

pub struct Preset {
    pub id: &'static str,
    pub ar: &'static str,
    pub en: &'static str,
    pub rows: &'static [&'static [Slot]],
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "wide-2", ar: "عريض و2 صغار", en: "1 wide, 2 small", rows: &[&[Wide], &[Small, Small]] },
    Preset { id: "wide-4", ar: "عريض و4 صغار", en: "1 wide, 4 small", rows: &[&[Wide], &[Small, Small], &[Small, Small]] },
    Preset { id: "2wide-2", ar: "2 عريض و2 صغار", en: "2 wide, 2 small", rows: &[&[Wide], &[Small, Small], &[Wide]] },
    Preset { id: "2wide-4", ar: "2 عريض و4 صغار", en: "2 wide, 4 small", rows: &[&[Wide], &[Small, Small], &[Wide], &[Small, Small]] },
    Preset { id: "small-4", ar: "4 صغار", en: "4 small", rows: &[&[Small, Small], &[Small, Small]] },
    Preset { id: "wide-3", ar: "3 عريض", en: "3 wide", rows: &[&[Wide], &[Wide], &[Wide]] },
];

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

impl Preset {
    pub fn slots(&self) -> Vec<Slot> {
        self.rows.iter().flat_map(|r| r.iter().copied()).collect()
    }
}

/// A widget's slot, by its width.
pub fn kind(size: (f32, f32)) -> Slot {
    if size.0 > 200.0 { Wide } else { Small }
}

/// Each widget's offset (DIPs) from the layout's top-left, for rows of widget sizes; and the
/// layout's size. Rows are as tall as their tallest; within a row the first is on the right.
pub fn place(rows: &[Vec<(f32, f32)>]) -> (Vec<Vec<(f32, f32)>>, (f32, f32)) {
    let row_w = |r: &Vec<(f32, f32)>| r.iter().map(|s| s.0).sum::<f32>() + GAP * (r.len().saturating_sub(1)) as f32;
    let width = rows.iter().map(row_w).fold(0.0, f32::max);
    let mut y = 0.0;
    let mut out = vec![];
    for r in rows {
        let mut x = width;
        let mut row = vec![];
        for s in r {
            x -= s.0;
            row.push((x, y));
            x -= GAP;
        }
        y += r.iter().map(|s| s.1).fold(0.0, f32::max) + GAP;
        out.push(row);
    }
    (out, (width, (y - GAP).max(0.0)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_line_up() {
        let (w, s) = ((370.0, 176.0), (176.0, 176.0));
        let (o, size) = place(&[vec![w], vec![s, s], vec![w]]);
        assert_eq!(o[0], vec![(0.0, 0.0)]);
        // two small fill the wide one's width, the first on the right
        assert_eq!(o[1], vec![(194.0, 194.0), (0.0, 194.0)]);
        assert_eq!(o[2], vec![(0.0, 388.0)]);
        assert_eq!(size, (370.0, 564.0));
        assert_eq!(preset("2wide-4").unwrap().slots().len(), 6);
        assert_eq!(kind((370.0, 176.0)), Slot::Wide);
    }
}
