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
| Undo last review | ❌ | backend `undo()` exists |
| Suspend / bury / flag / mark | ❌ | backend ops exist |
| Edit current note | ❌ | needs editor screen |
| Audio replay key | ❌ | with media playback |

## Browser

| Capability | Status | Notes |
|---|---|---|
| Search (Anki query language) | ✅ | `search_cards` + list, cap 200 |
| Sort / columns | ❌ | creation order only |
| Select + bulk suspend/bury/delete | ✅ | + unsuspend, two-click delete confirm |
| Bulk tags / change deck / reposition | ✅ | tags+deck done; reposition later |
| Find & replace | ❌ | |
| Card info / preview | ❌ | `card_stats` RPC exists |
| Sidebar (decks/tags/flags) | ❌ | |

## Editor / Add

| Capability | Status | Notes |
|---|---|---|
| Edit fields (plain) | ❌ | after browser |
| Rich text (bold/italic/color) | ❌ | stored as HTML, render as-is |
| Cloze button | ❌ | text insertion, no preview magic |
| MathJax render | ❌ | webviews only; TUI shows source |
| Attach image/audio | ❌ | with media work |
| Notetype / deck pickers | ❌ | |
| Template editor | ❌ | later; low frequency |
| Image occlusion | ❌ | later; specialized flow |

## Decks & scheduling

| Capability | Status | Notes |
|---|---|---|
| Deck list | ✅ | counts in v1.1 |
| Deck overview (new/learn/review counts) | ❌ | `QueuedCards` counts exist |
| Options / presets (full) | ❌ | 30+ fields; big screen |
| Add / rename / delete deck | ❌ | |
| Filtered decks + custom study | ❌ | |
| Import / export `.apkg` | ❌ | backend exists |

## Stats

| Capability | Status | Notes |
|---|---|---|
| Answer-time (mean/deviations/slowest/heatmap) | ✅ | our edge over Anki |
| Full graphs page (30 graphs) | ❌ | `Graphs` RPC exists, needs UI |

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
7. Full stats graphs, preferences, maintenance tools.
