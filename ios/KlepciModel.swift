import SwiftUI

// Uncomment when `klep_mobile.swift` is generated (see ios/README.md):
// import klep_mobile

/// Replace `AnyMobileCore` with the generated `MobileCore` type.
final class KlepciModel: ObservableObject {
    @Published var decks: [DeckRow] = []
    @Published var card: (id: Int64, question: String)? = nil
    @Published var answer: String? = nil
    @Published var done = false
    @Published var marked = false
    @Published var flagValue = 0
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
        // let rows = try? core.deckOverview()
        // decks = (rows ?? []).map {
        //     DeckRow(id: $0.id, name: $0.name, level: Int($0.level),
        //             newCount: Int($0.new), learnCount: Int($0.learn),
        //             reviewCount: Int($0.review), total: Int($0.total),
        //             filtered: $0.filtered)
        // }
        decks = []
    }

    @Published var deckEditName = ""
    @Published var deckEditId: Int64? = nil
    @Published var showDeckEditor = false
    @Published var deckStatus = ""

    func createDeck() {
        // deckStatus = "created #\(try? core.addDeck(name: deckEditName))"
        deckEditName = ""
        showDeckEditor = false
        refreshDecks()
    }

    func renameDeck() {
        // if let id = deckEditId { try? core.renameDeck(deckId: id, name: deckEditName) }
        deckEditId = nil
        showDeckEditor = false
        refreshDecks()
    }

    func deleteDeck(_ id: Int64) {
        // try? core.deleteDeck(deckId: id)
        refreshDecks()
    }

    func selectDeck(_ id: Int64) {
        // try? core.selectDeck(deckId: id)
        _ = id
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

    func undo() {
        // try? core.undo()
        next()
    }

    func suspendCurrent() {
        // if let id = card?.id { try? core.suspend(ids: [id], suspend: true) }
        next()
    }

    func buryCurrent() {
        // if let id = card?.id { try? core.bury(ids: [id]) }
        next()
    }

    func toggleMark() {
        // if let id = card?.id { marked = (try? core.toggleMark(cardId: id)) ?? marked }
    }

    func setFlag(_ flag: UInt32) {
        // if let id = card?.id { try? core.flagCard(cardId: id, flag: flag) }
        flagValue = Int(flag)
    }

    func loadStats() {
        // parse try? core.timingJson(search: "", topN: 10)
    }

    // Editor state (wired to editData/saveNote/addNote/notetypeNames/
    // fieldNames once klep_mobile.swift is integrated).
    @Published var editingNote: Int64? = nil
    @Published var editFields: [String] = []
    @Published var editNames: [String] = []
    @Published var editTags = ""
    @Published var addNotetype = ""
    @Published var addDeck = ""
    @Published var addFields: [String] = []
    @Published var addNames: [String] = []
    @Published var addTags = ""
    @Published var addStatus = ""

    func startEdit(cardId: Int64) {
        // let data = try? core.editData(cardId: cardId)
        // editingNote = data?.noteId; editFields = data?.fields ?? [] ...
        _ = cardId
    }

    func saveEdit() {
        // try? core.saveNote(noteId: editingNote!, fields: editFields, tags: editTags)
        editingNote = nil
    }

    func saveAdd() {
        // let id = try? core.addNote(...)
        // addStatus = "added #\(id)"; clear fields
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
