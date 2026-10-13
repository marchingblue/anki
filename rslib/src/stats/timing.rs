// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Deck/scope-wide answer-timing statistics, computed with the
//! `klepci-stats` crate (Polars) over revlog rows.

use std::collections::HashMap;

use klepci_stats::RevlogRow;

use crate::prelude::*;
use crate::search::SortMode;

impl Collection {
    /// Deck/scope-wide answer-timing stats. Public klepci API (the
    /// collection format is untouched; this only reads revlogs).
    pub fn timing_stats_for_search(
        &mut self,
        search: &str,
        top_n: u32,
    ) -> Result<anki_proto::stats::TimingStatsResponse> {
        let top_n = if top_n == 0 { 20 } else { top_n.min(200) } as usize;
        let guard = self.search_cards_into_table(search, SortMode::NoOrder)?;
        let col = &mut *guard.col;
        let revlog = col.storage.get_revlog_entries_for_searched_cards()?;
        let decks: HashMap<CardId, DeckId> = col
            .storage
            .all_searched_cards()?
            .into_iter()
            .map(|card| (card.id, card.deck_id))
            .collect();

        let rows: Vec<RevlogRow> = revlog
            .iter()
            .map(|entry| RevlogRow {
                card_id: entry.cid.0,
                deck_id: decks.get(&entry.cid).map(|deck| deck.0).unwrap_or(0),
                timestamp_secs: entry.id.0 / 1000,
                kind: entry.review_kind as u8,
                button_chosen: entry.button_chosen,
                ease_factor: entry.ease_factor,
                taken_millis: entry.taken_millis,
            })
            .collect();

        let frame = klepci_stats::frame(&rows).or_invalid("timing frame")?;
        let deck = klepci_stats::deck_timing(&frame).or_invalid("deck timing")?;
        let cards = klepci_stats::per_card_stats(&frame).or_invalid("per-card timing")?;
        let slowest = klepci_stats::top_by_mean(cards.clone(), top_n);
        let outliers = klepci_stats::with_zscores(cards, &deck);
        let days = klepci_stats::heatmap(&frame).or_invalid("timing heatmap")?;

        use anki_proto::stats::timing_stats_response::{CardTiming, DayBucket};
        let to_card_timing = |c: klepci_stats::PerCardStats| CardTiming {
            card_id: c.card_id,
            reviews: c.reviews,
            total_secs: c.total_secs as f32,
            mean_secs: c.mean_secs as f32,
            median_secs: c.median_secs as f32,
            std_secs: c.std_secs as f32,
            min_secs: c.min_secs as f32,
            max_secs: c.max_secs as f32,
            z_vs_deck: c.z_vs_deck as f32,
        };
        Ok(anki_proto::stats::TimingStatsResponse {
            reviews: deck.reviews,
            total_secs: deck.total_secs as f32,
            mean_secs: deck.mean_secs as f32,
            std_secs: deck.std_secs as f32,
            slowest: slowest.into_iter().map(to_card_timing).collect(),
            outliers: outliers
                .into_iter()
                .take(top_n)
                .map(to_card_timing)
                .collect(),
            days: days
                .into_iter()
                .map(|day| DayBucket {
                    day: day.day,
                    reviews: day.reviews,
                    total_secs: day.total_secs as f32,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn reviewed_collection() -> Result<Collection> {
        let mut col = Collection::new();
        let nt = col.get_notetype_by_name("Basic")?.unwrap();
        for _ in 0..2 {
            let mut note = nt.new_note();
            col.add_note(&mut note, DeckId(1))?;
        }
        let cids = col.search_cards("", SortMode::NoOrder)?;
        assert_eq!(cids.len(), 2);
        // Revlog ids are millisecond timestamps; keep answers in
        // distinct milliseconds so the ids can't collide.
        col.grade_now(&cids[0..1], 3)?;
        std::thread::sleep(std::time::Duration::from_millis(5));
        col.grade_now(&cids[1..2], 3)?;
        Ok(col)
    }

    #[test]
    fn timing_stats_wiring() -> Result<()> {
        let mut col = reviewed_collection()?;
        let resp = col.timing_stats_for_search("", 0)?;
        assert_eq!(resp.reviews, 2);
        assert_eq!(resp.slowest.len(), 2);
        assert_eq!(resp.outliers.len(), 2);
        assert_eq!(resp.days.len(), 1);
        assert_eq!(resp.days[0].reviews, 2);
        // test answers are instant, so timings are zero — the math itself
        // is covered by klepci-stats unit tests with synthetic nonzero rows
        assert_eq!(resp.total_secs, 0.0);
        assert!(resp.slowest.iter().all(|c| c.reviews == 1));
        Ok(())
    }

    #[test]
    fn timing_stats_search_scopes() -> Result<()> {
        let mut col = reviewed_collection()?;
        let resp = col.timing_stats_for_search("nonexistenttokenxyz", 10)?;
        assert_eq!(resp.reviews, 0);
        assert!(resp.slowest.is_empty());
        Ok(())
    }
}
