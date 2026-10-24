# Frontend parity with stock Anki

Rule: everything doable in stock Anki must become doable in klepci
frontends. Backend ops already exist (we reuse `rslib`) — parity is
frontend + `klep-core` wiring work, not new algorithms.

Legend: ✅ done · 🚧 in progress · ❌ not started · ➖ intentionally different

## Reviewer

| Capability | Status | Notes |
|---|---|---|
| Question/answer render | ✅ | text-only v1, media placeholders |
| Again/Hard/Good/Easy | ✅ | + keyboard 1-4/space (desktop/TUI) |
| Answer timer | ✅ | measured, stored in revlog |
| Media render (img/audio) | ❌ | v1.1: `media_bytes` + playback |
| Undo last review | ✅ | undoes any last op; review resumes |
| Suspend / bury / flag / mark | ✅ | susp/bury, flags 0-7, Anki `marked` tag |
| Edit current note | ✅ | desktop form, TUI `$EDITOR`, mobile sheet/dialog |
| Audio replay key | ❌ | with media playback |

## Browser

| Capability | Status | Notes |
|---|---|---|
| Search (Anki query language) | ✅ | `search_cards` + list, cap 200 |
| Sort / columns | ❌ | creation order only |
| Select + bulk suspend/bury/delete | ✅ | + unsuspend, two-click delete confirm |
| Bulk tags / change deck / reposition | ✅ | tags+deck done; reposition later |
| Find & replace | ✅ | regex, optional field, current search scope |
| Card info / preview | ❌ | `card_stats` RPC exists |
| Sidebar (decks/tags/flags) | ❌ | deck filter via search syntax for now |
| Sort / columns | ❌ | creation order only |

## Editor / Add

| Capability | Status | Notes |
|---|---|---|
| Edit fields (plain) | ✅ | desktop form, TUI via `$EDITOR`, mobile native fields |
| Add note (type/deck/fields/tags) | ✅ | same coverage all frontends |
| Rich text (bold/italic/color) | ❌ | stored as HTML, render as-is |
| Cloze button | ❌ | text insertion, no preview magic |
| MathJax render | ❌ | webviews only; TUI shows source |
| Attach image/audio | ❌ | with media work |
| Notetype / deck pickers | ✅ | dropdowns/lists on all frontends |
| Template editor | ❌ | later; low frequency |
| Image occlusion | ❌ | later; specialized flow |

## Decks & scheduling

| Capability | Status | Notes |
|---|---|---|
| Deck list | ✅ | with live due counts, indent, filtered marker |
| Deck overview (new/learn/review counts) | ✅ | today's counts, children summed |
| Options / presets | 🚧 | key scheduling/FSRS fields on all frontends; advanced orders/custom scheduling later |
| Add / rename / delete deck | ✅ | two-click delete; children follow rename/delete |
| Deck-scoped review | ✅ | selecting a deck sets the review queue |
| Filtered decks + custom study | ❌ | |
| Import / export `.apkg` | ✅ | stock importer/exporter, media included |

## Stats

| Capability | Status | Notes |
|---|---|---|
| Answer-time (mean/deviations/slowest/heatmap) | ✅ | our edge over Anki |
| Full graphs page (30 graphs) | 🚧 | reviews/day, added, future due, intervals, ease, hours, buttons, retention, card counts on every frontend; FSRS simulator/workload still missing |

## System

| Capability | Status | Notes |
|---|---|---|
| Preferences | ❌ | later |
| Profiles | ➖ | one profile per device, by design |
| AnkiWeb sync | ➖ | our own sync instead (bundle sync) |
| Python add-ons | ➖ | no runtime; agent API covers automation |
| Check database / check media / empty cards | ❌ | backend exists |

## Order of attack

1. Browser (search + list + select + bulk basics) — unlocks real use.
2. Editor (plain fields) + Add note.
3. Deck counts/overview + deck CRUD.
4. Undo/suspend/bury/flag in reviewer.
5. Media render + attach.
6. Options/presets, filtered decks, import/export UI.
7. Remaining graphs (FSRS simulator/workload), preferences, maintenance tools.
