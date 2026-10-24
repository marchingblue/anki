import SwiftUI

struct DecksView: View {
    @ObservedObject var model: KlepciModel
    var goReview: () -> Void
    @State private var confirmDelete: Int64? = nil

    var body: some View {
        List {
            Section {
                Button("New deck") {
                    model.deckEditName = ""
                    model.deckEditId = nil
                    model.showDeckEditor = true
                }
                if !model.deckStatus.isEmpty {
                    Text(model.deckStatus).font(.caption).foregroundStyle(.secondary)
                }
            }
            ForEach(model.decks) { deck in
                VStack(alignment: .leading, spacing: 2) {
                    Button((deck.filtered ? "▣ " : "") + deck.name) {
                        model.selectDeck(deck.id)
                        goReview()
                    }
                        .font(.body.weight(.semibold))
                    Text("\(deck.newCount) new · \(deck.learnCount) learn · " +
                         "\(deck.reviewCount) review · \(deck.total) cards")
                        .font(.caption)
                        .foregroundStyle(deck.newCount + deck.learnCount + deck.reviewCount == 0
                                         ? .secondary : Color.accentColor)
                    HStack {
                        Button("Rename") {
                            model.deckEditName = deck.name
                            model.deckEditId = deck.id
                            model.showDeckEditor = true
                        }
                        .font(.caption)
                        Button(confirmDelete == deck.id ? "Confirm delete" : "Delete",
                               role: .destructive) {
                            if confirmDelete == deck.id {
                                model.deleteDeck(deck.id)
                                confirmDelete = nil
                            } else {
                                confirmDelete = deck.id
                            }
                        }
                        .font(.caption)
                    }
                }
                .padding(.leading, CGFloat(max(0, deck.level - 1)) * 14)
            }
        }
        .navigationTitle("Decks")
        .onAppear { model.refreshDecks() }
        .sheet(isPresented: $model.showDeckEditor) {
            NavigationStack {
                Form {
                    TextField("Deck name (A::B for subdecks)", text: $model.deckEditName)
                }
                .navigationTitle(model.deckEditId == nil ? "New deck" : "Rename deck")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { model.showDeckEditor = false }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save") {
                            if model.deckEditId == nil { model.createDeck() }
                            else { model.renameDeck() }
                        }
                    }
                }
            }
        }
    }
}

struct ReviewView: View {
    @ObservedObject var model: KlepciModel

    var body: some View {
        Group {
            if model.done {
                VStack { Text("done for now 🎉").font(.title2) }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let card = model.card {
                ScrollView {
                    Text(htmlToText(card.question)).font(.title3).frame(maxWidth: .infinity, alignment: .leading)
                    if let answer = model.answer {
                        Divider()
                        Text(htmlToText(answer)).font(.title3).frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                .padding()
                HStack {
                    Button("Undo") { model.undo() }
                    Button("Suspend") { model.suspendCurrent() }
                    Button("Bury") { model.buryCurrent() }
                    Button(model.marked ? "Unmark" : "Mark") { model.toggleMark() }
                    Menu("Flag \(model.flagValue)") {
                        ForEach(0..<8, id: \.self) { value in
                            Button("\(value)") { model.setFlag(UInt32(value)) }
                        }
                    }
                }
                .buttonStyle(.bordered)
                .font(.caption)
                if model.answer == nil {
                    Button("Show answer") { model.show() }
                        .buttonStyle(.borderedProminent)
                        .padding()
                } else {
                    HStack {
                        ForEach([(1, "Again"), (2, "Hard"), (3, "Good"), (4, "Easy")], id: \.0) { rating, label in
                            Button(label) { model.rate(UInt8(rating)) }
                                .buttonStyle(.bordered)
                                .frame(maxWidth: .infinity)
                        }
                    }
                    .padding()
                }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle("Review")
        .onAppear { if model.card == nil { model.next() } }
        .toolbar {
            if model.card != nil {
                Button("Edit") {
                    if let id = model.card?.id { model.startEdit(cardId: id) }
                }
            }
        }
        .sheet(isPresented: Binding(
            get: { model.editingNote != nil },
            set: { if !$0 { model.editingNote = nil } }
        )) {
            NavigationStack {
                Form {
                    ForEach(model.editNames.indices, id: \.self) { i in
                        TextField(model.editNames[i], text: $model.editFields[i], axis: .vertical)
                    }
                    TextField("tags", text: $model.editTags)
                }
                .navigationTitle("Edit note")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { model.editingNote = nil }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save") { model.saveEdit() }
                    }
                }
            }
        }
    }
}

struct BrowseView: View {
    @ObservedObject var model: KlepciModel

    var body: some View {
        VStack {
            HStack {
                TextField("search", text: $model.query)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit { model.runSearch() }
                Button("Go") { model.runSearch() }
            }
            .padding(.horizontal)
            HStack {
                Button("All") { model.selected = Set(model.rows.map(\.id)) }
                Button("Susp") { /* try? core.suspendCards(ids: suspend: true) */ model.runSearch() }
                Button("Uns") { /* try? core.suspendCards(ids: suspend: false) */ model.runSearch() }
                Button("Bury") { /* try? core.buryCards(ids:) */ model.runSearch() }
                Button(model.confirmDelete ? "Sure?" : "Del") {
                    if model.confirmDelete {
                        // try? core.deleteCards(ids:)
                        model.runSearch()
                    } else {
                        model.confirmDelete = true
                    }
                }
                Text("\(model.selected.count) sel").foregroundStyle(.secondary)
            }
            .buttonStyle(.bordered)
            .padding(.horizontal)
            List(model.rows, id: \.id) { row in
                HStack {
                    Image(systemName: model.selected.contains(row.id) ? "checkmark.square" : "square")
                    Text("[\(row.deck)] \(row.preview)").lineLimit(2)
                }
                .contentShape(Rectangle())
                .onTapGesture {
                    if model.selected.contains(row.id) {
                        model.selected.remove(row.id)
                    } else {
                        model.selected.insert(row.id)
                    }
                }
            }
        }
        .navigationTitle("Browse")
    }
}

struct AddView: View {
    @ObservedObject var model: KlepciModel

    var body: some View {
        Form {
            TextField("Notetype", text: $model.addNotetype)
            TextField("Deck", text: $model.addDeck)
            ForEach(model.addNames.indices, id: \.self) { i in
                TextField(model.addNames[i], text: $model.addFields[i], axis: .vertical)
            }
            TextField("tags", text: $model.addTags)
            Button("Add note") { model.saveAdd() }
            if !model.addStatus.isEmpty {
                Text(model.addStatus).foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Add")
    }
}

struct StatsView: View {
    @ObservedObject var model: KlepciModel

    var body: some View {
        List {
            Section("Answer time") { Text(model.summary) }
            Section("Heat") {
                HeatGrid(days: model.heat)
                    .frame(height: 120)
            }
            Section("Slowest cards") {
                ForEach(model.slowest) { card in
                    Text("#\(card.id) · \(card.reps) reps · mean \(formatSecs(card.mean)) · worst \(formatSecs(card.worst))")
                        .font(.callout)
                }
            }
        }
        .navigationTitle("Stats")
        .onAppear { model.loadStats() }
    }
}

struct HeatGrid: View {
    let days: [(day: Int64, reviews: Int)]

    var body: some View {
        GeometryReader { geo in
            let recent = Array(days.suffix(182))
            let max = max(1, recent.map(\.reviews).max() ?? 1)
            let cell = min(geo.size.width / 27, geo.size.height / 7)
            ForEach(0..<recent.count, id: \.self) { i in
                let t = Double(recent[i].reviews) / Double(max)
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color(red: 0.12 + t * 0.36, green: 0.13 + t * 0.5, blue: 0.16 + t * 0.8))
                    .frame(width: cell - 2, height: cell - 2)
                    .position(
                        x: CGFloat(i / 7) * cell + cell / 2,
                        y: CGFloat(i % 7) * cell + cell / 2
                    )
            }
        }
    }
}
