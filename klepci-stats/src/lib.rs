// Copyright: klepci contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Timing statistics over Anki revlog rows, powered by Polars.
//!
//! The input is a slice of [`RevlogRow`] — a trimmed mirror of
//! `rslib/src/revlog/mod.rs::RevlogEntry` carrying only what timing stats
//! need. A later step will add `From<&RevlogEntry>` once this crate is
//! attached to the anki workspace.
//!
//! Filtering mirrors `RevlogEntry::has_rating_and_affects_scheduling`:
//! entries with no rating (manual reschedules, resets) and cramming entries
//! (filtered deck, no reschedule) are excluded. Note this is stricter than
//! the per-card average shown in stock Anki, which counts by `has_rating`
//! only.
//!
//! All JSON output is compact (`serde_json::to_string`), not pretty —
//! agents pay per token.

use polars::prelude::*;
use serde::Serialize;

/// Discriminants mirror `RevlogReviewKind` in `rslib/src/revlog/mod.rs`.
pub mod kind {
    pub const LEARNING: u8 = 0;
    pub const REVIEW: u8 = 1;
    pub const RELEARNING: u8 = 2;
    pub const FILTERED: u8 = 3;
    pub const MANUAL: u8 = 4;
    pub const RESCHEDULED: u8 = 5;
}

/// One revlog entry, trimmed to timing-relevant fields.
#[derive(Debug, Clone, Copy)]
pub struct RevlogRow {
    pub card_id: i64,
    pub deck_id: i64,
    pub timestamp_secs: i64,
    pub kind: u8,
    pub button_chosen: u8,
    pub ease_factor: u32,
    pub taken_millis: u32,
}

/// Mirror of `RevlogEntry::has_rating_and_affects_scheduling`.
pub fn is_counted(row: &RevlogRow) -> bool {
    row.button_chosen > 0 && !(row.kind == kind::FILTERED && row.ease_factor == 0)
}

/// Build the working frame. Only counted rows are kept.
/// Columns: `card_id: i64`, `deck_id: i64`, `day: i64` (epoch days),
/// `taken_secs: f64`.
pub fn frame(rows: &[RevlogRow]) -> PolarsResult<DataFrame> {
    let cids: Vec<i64> = rows
        .iter()
        .filter(|r| is_counted(r))
        .map(|r| r.card_id)
        .collect();
    let decks: Vec<i64> = rows
        .iter()
        .filter(|r| is_counted(r))
        .map(|r| r.deck_id)
        .collect();
    let days: Vec<i64> = rows
        .iter()
        .filter(|r| is_counted(r))
        .map(|r| r.timestamp_secs / 86_400)
        .collect();
    let secs: Vec<f64> = rows
        .iter()
        .filter(|r| is_counted(r))
        .map(|r| r.taken_millis as f64 / 1000.0)
        .collect();
    df!(
        "card_id" => cids,
        "deck_id" => decks,
        "day" => days,
        "taken_secs" => secs,
    )
}

/// Per-card aggregates over counted reviews.
pub fn per_card_frame(df: &DataFrame) -> PolarsResult<DataFrame> {
    df.clone()
        .lazy()
        .group_by([col("card_id")])
        .agg([
            col("taken_secs").len().alias("reviews"),
            col("taken_secs").sum().alias("total_secs"),
            col("taken_secs").mean().alias("mean_secs"),
            col("taken_secs").median().alias("median_secs"),
            col("taken_secs").std(1).alias("std_secs"),
            col("taken_secs").min().alias("min_secs"),
            col("taken_secs").max().alias("max_secs"),
        ])
        .collect()
}

/// Deck-level totals over counted reviews.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct DeckTiming {
    pub reviews: u32,
    pub total_secs: f64,
    pub mean_secs: f64,
    pub std_secs: f64,
}

pub fn deck_timing(df: &DataFrame) -> PolarsResult<DeckTiming> {
    let agg = df
        .clone()
        .lazy()
        .select([
            col("taken_secs").len().alias("n"),
            col("taken_secs").sum().alias("sum"),
            col("taken_secs").mean().alias("mean"),
            col("taken_secs").std(1).alias("std"),
        ])
        .collect()?;
    if agg.height() == 0 {
        return Ok(DeckTiming::default());
    }
    let n = agg.column("n")?.u32()?.get(0).unwrap_or(0);
    let num = |name: &str| {
        agg.column(name)
            .ok()
            .and_then(|c| c.f64().ok())
            .and_then(|ca| ca.get(0))
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    Ok(DeckTiming {
        reviews: n,
        total_secs: num("sum"),
        mean_secs: num("mean"),
        std_secs: num("std"),
    })
}

fn num_or_zero(col: Option<&Column>, idx: usize) -> f64 {
    col.and_then(|c| c.f64().ok())
        .and_then(|ca| ca.get(idx))
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

/// Per-card stats as plain Rust, ready for ranking and JSON.
#[derive(Debug, Clone, Serialize)]
pub struct PerCardStats {
    pub card_id: i64,
    pub reviews: u32,
    pub total_secs: f64,
    pub mean_secs: f64,
    pub median_secs: f64,
    pub std_secs: f64,
    pub min_secs: f64,
    pub max_secs: f64,
    /// z-score of `mean_secs` against deck timing; filled by [`with_zscores`].
    pub z_vs_deck: f64,
}

pub fn per_card_stats(df: &DataFrame) -> PolarsResult<Vec<PerCardStats>> {
    let agg = per_card_frame(df)?;
    let cids = agg.column("card_id")?.i64()?;
    let reviews = agg.column("reviews")?.u32()?;
    let mut out = Vec::with_capacity(agg.height());
    for i in 0..agg.height() {
        out.push(PerCardStats {
            card_id: cids.get(i).unwrap_or(0),
            reviews: reviews.get(i).unwrap_or(0),
            total_secs: num_or_zero(agg.column("total_secs").ok(), i),
            mean_secs: num_or_zero(agg.column("mean_secs").ok(), i),
            median_secs: num_or_zero(agg.column("median_secs").ok(), i),
            std_secs: num_or_zero(agg.column("std_secs").ok(), i),
            min_secs: num_or_zero(agg.column("min_secs").ok(), i),
            max_secs: num_or_zero(agg.column("max_secs").ok(), i),
            z_vs_deck: 0.0,
        });
    }
    Ok(out)
}

/// Attach deck-relative z-scores and sort descending (worst outliers first).
pub fn with_zscores(mut cards: Vec<PerCardStats>, deck: &DeckTiming) -> Vec<PerCardStats> {
    for c in &mut cards {
        c.z_vs_deck = if deck.std_secs > 0.0 {
            (c.mean_secs - deck.mean_secs) / deck.std_secs
        } else {
            0.0
        };
    }
    cards.sort_by(|a, b| {
        b.z_vs_deck
            .partial_cmp(&a.z_vs_deck)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cards
}

/// Top-`n` cards by mean answer time ("which card takes longest").
pub fn top_by_mean(mut cards: Vec<PerCardStats>, n: usize) -> Vec<PerCardStats> {
    cards.sort_by(|a, b| {
        b.mean_secs
            .partial_cmp(&a.mean_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cards.truncate(n);
    cards
}

/// Top-`n` cards by single worst review.
pub fn top_by_max(mut cards: Vec<PerCardStats>, n: usize) -> Vec<PerCardStats> {
    cards.sort_by(|a, b| {
        b.max_secs
            .partial_cmp(&a.max_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cards.truncate(n);
    cards
}

/// Reviews per epoch day — the heatmap source.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DayBucket {
    pub day: i64,
    pub reviews: u32,
    pub total_secs: f64,
}

pub fn heatmap(df: &DataFrame) -> PolarsResult<Vec<DayBucket>> {
    let agg = df
        .clone()
        .lazy()
        .group_by([col("day")])
        .agg([
            col("taken_secs").len().alias("reviews"),
            col("taken_secs").sum().alias("total_secs"),
        ])
        .collect()?;
    let days = agg.column("day")?.i64()?;
    let reviews = agg.column("reviews")?.u32()?;
    let mut out = Vec::with_capacity(agg.height());
    for i in 0..agg.height() {
        out.push(DayBucket {
            day: days.get(i).unwrap_or(0),
            reviews: reviews.get(i).unwrap_or(0),
            total_secs: num_or_zero(agg.column("total_secs").ok(), i),
        });
    }
    out.sort_by_key(|b| b.day);
    Ok(out)
}

/// Compact JSON for agents and the TUI.
pub fn to_json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

#[cfg(test)]
mod test {
    use super::*;
    use kind::*;

    fn rows() -> Vec<RevlogRow> {
        vec![
            // card 1: fast, 3 reviews (10s, 20s, 30s)
            RevlogRow { card_id: 1, deck_id: 7, timestamp_secs: 86_400 * 10 + 5, kind: REVIEW, button_chosen: 3, ease_factor: 250, taken_millis: 10_000 },
            RevlogRow { card_id: 1, deck_id: 7, timestamp_secs: 86_400 * 11, kind: REVIEW, button_chosen: 3, ease_factor: 250, taken_millis: 20_000 },
            RevlogRow { card_id: 1, deck_id: 7, timestamp_secs: 86_400 * 11 + 9, kind: LEARNING, button_chosen: 1, ease_factor: 0, taken_millis: 30_000 },
            // card 2: slow, 2 reviews (100s, 200s)
            RevlogRow { card_id: 2, deck_id: 7, timestamp_secs: 86_400 * 10, kind: REVIEW, button_chosen: 2, ease_factor: 200, taken_millis: 100_000 },
            RevlogRow { card_id: 2, deck_id: 7, timestamp_secs: 86_400 * 12, kind: REVIEW, button_chosen: 4, ease_factor: 300, taken_millis: 200_000 },
            // excluded: manual reschedule (no rating)
            RevlogRow { card_id: 2, deck_id: 7, timestamp_secs: 86_400 * 12, kind: MANUAL, button_chosen: 0, ease_factor: 250, taken_millis: 0 },
            // excluded: cramming (filtered, ease 0)
            RevlogRow { card_id: 1, deck_id: 7, timestamp_secs: 86_400 * 12, kind: FILTERED, button_chosen: 3, ease_factor: 0, taken_millis: 5_000 },
        ]
    }

    #[test]
    fn filters_manual_and_cramming() {
        let df = frame(&rows()).unwrap();
        assert_eq!(df.height(), 5);
    }

    #[test]
    fn per_card_means() {
        let df = frame(&rows()).unwrap();
        let mut cards = per_card_stats(&df).unwrap();
        cards.sort_by_key(|c| c.card_id);
        assert_eq!(cards.len(), 2);
        assert!((cards[0].mean_secs - 20.0).abs() < 1e-9);
        assert!((cards[1].mean_secs - 150.0).abs() < 1e-9);
        assert_eq!(cards[0].reviews, 3);
        assert!((cards[0].total_secs - 60.0).abs() < 1e-9);
        assert!((cards[1].max_secs - 200.0).abs() < 1e-9);
        assert!((cards[0].min_secs - 10.0).abs() < 1e-9);
    }

    #[test]
    fn deck_and_zscores() {
        let df = frame(&rows()).unwrap();
        let deck = deck_timing(&df).unwrap();
        assert_eq!(deck.reviews, 5);
        assert!((deck.total_secs - 360.0).abs() < 1e-9);
        assert!((deck.mean_secs - 72.0).abs() < 1e-9);
        let cards = per_card_stats(&df).unwrap();
        let ranked = with_zscores(cards, &deck);
        // card 2 is slower than the deck mean -> positive z, first
        assert_eq!(ranked[0].card_id, 2);
        assert!(ranked[0].z_vs_deck > 0.0);
        assert!(ranked[1].z_vs_deck < 0.0);
        // top-by-mean agrees
        let top = top_by_mean(per_card_stats(&df).unwrap(), 1);
        assert_eq!(top[0].card_id, 2);
    }

    #[test]
    fn heatmap_days() {
        let df = frame(&rows()).unwrap();
        let heat = heatmap(&df).unwrap();
        assert_eq!(heat.len(), 3);
        assert_eq!(heat[0].day, 10);
        assert_eq!(heat[0].reviews, 2);
        assert_eq!(heat[1].day, 11);
        assert_eq!(heat[1].reviews, 2);
        assert_eq!(heat[2].day, 12);
        assert_eq!(heat[2].reviews, 1);
    }

    #[test]
    fn empty_input() {
        let df = frame(&[]).unwrap();
        assert_eq!(df.height(), 0);
        assert!(per_card_stats(&df).unwrap().is_empty());
        assert!(heatmap(&df).unwrap().is_empty());
        let deck = deck_timing(&df).unwrap();
        assert_eq!(deck.reviews, 0);
    }

    #[test]
    fn json_is_compact() {
        let df = frame(&rows()).unwrap();
        let cards = per_card_stats(&df).unwrap();
        let json = to_json(&cards);
        assert!(!json.contains('\n'));
        assert!(json.contains("\"card_id\":1"));
    }
}
