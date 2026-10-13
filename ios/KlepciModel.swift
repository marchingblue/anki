import SwiftUI

// Uncomment when `klep_mobile.swift` is generated (see ios/README.md):
// import klep_mobile

/// Replace `AnyMobileCore` with the generated `MobileCore` type.
final class KlepciModel: ObservableObject {
    @Published var decks: [DeckRow] = []
    @Published var card: (id: Int64, question: String)? = nil
    @Published var answer: String? = nil
    @Published var done = false
    @Published var error: String? = nil
    @Published var heat: [(day: Int64, reviews: Int)] = []
    @Published var slowest: [SlowCard] = []
    @Published var summary: String = ""

    private var shownAt = Date()

    init() {
        // let path = documents/collection.anki2
        // self.core = try! MobileCore.open(path: path)
        refreshDecks()
    }

    func refreshDecks() {
        // decks = (try? core.decks().map { DeckRow(id: $0.id, name: $0.name) }) ?? []
        decks = []
    }

    func next() {
        answer = nil
        // if let card = try? core.nextCard() { ... } else { done = true }
        done = decks.isEmpty
    }

    func show() {
        // answer = htmlToText((try? core.showAnswer(cardId: id)?.answer) ?? "")
        shownAt = Date()
    }

    func rate(_ rating: UInt8) {
        // let secs = UInt32(max(1, Int(Date().timeIntervalSince(shownAt))))
        // try? core.answer(cardId: id, rating: rating, secs: secs)
        next()
    }

    func loadStats() {
        // parse try? core.timingJson(search: "", topN: 10)
    }

    // Browse state (wired to searchCards/suspendCards/buryCards/deleteCards
    // once klep_mobile.swift is integrated).
    @Published var query = ""
    @Published var rows: [(id: Int64, deck: String, preview: String)] = []
    @Published var selected = Set<Int64>()
    @Published var confirmDelete = false

    func runSearch() {
        // rows = (try? core.searchCards(query: query, limit: 200))?.map { ... } ?? []
        selected = []
        confirmDelete = false
    }
}
