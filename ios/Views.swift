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
                        Button("Options") { model.openOptions(deckId: deck.id) }
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
        .sheet(isPresented: $model.showOptions) {
            NavigationStack {
                Form {
                    if let options = model.deckOptions {
                        Text("Changes apply to every deck using this preset.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        TextField("New cards/day", value: Binding(
                            get: { options.newPerDay },
                            set: { model.deckOptions?.newPerDay = $0 }
                        ), format: .number)
                        TextField("Reviews/day", value: Binding(
                            get: { options.reviewsPerDay },
                            set: { model.deckOptions?.reviewsPerDay = $0 }
                        ), format: .number)
                        TextField("Learning steps", text: Binding(
                            get: { options.learnSteps },
                            set: { model.deckOptions?.learnSteps = $0 }
                        ))
                        TextField("Relearning steps", text: Binding(
                            get: { options.relearnSteps },
                            set: { model.deckOptions?.relearnSteps = $0 }
                        ))
                        TextField("Easy interval", value: Binding(
                            get: { options.graduatingIntervalEasy },
                            set: { model.deckOptions?.graduatingIntervalEasy = $0 }
                        ), format: .number)
                        TextField("Maximum interval", value: Binding(
                            get: { options.maximumReviewInterval },
                            set: { model.deckOptions?.maximumReviewInterval = $0 }
                        ), format: .number)
                        TextField("Desired retention", value: Binding(
                            get: { options.desiredRetention },
                            set: { model.deckOptions?.desiredRetention = $0 }
                        ), format: .number)
                        Toggle("FSRS scheduler", isOn: Binding(
                            get: { options.fsrs },
                            set: { model.deckOptions?.fsrs = $0 }
                        ))
                        Toggle("Show answer timer", isOn: Binding(
                            get: { options.showTimer },
                            set: { model.deckOptions?.showTimer = $0 }
                        ))
                    }
                }
                .navigationTitle("Options")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { model.showOptions = false }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save") { model.saveOptions() }
                    }
                }
            }
        }
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
                Button("Replace") { model.showReplace = true }
            }
            .padding(.horizontal)
            if !model.replaceStatus.isEmpty {
                Text(model.replaceStatus).font(.caption).foregroundStyle(.secondary)
            }
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
        .sheet(isPresented: $model.showReplace) {
            NavigationStack {
                Form {
                    TextField("find (regex)", text: $model.replaceRegex)
                    TextField("replace with", text: $model.replaceWith)
                    TextField("field name (optional)", text: $model.replaceField)
                }
                .navigationTitle("Find & replace")
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { model.showReplace = false }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Replace all") { model.doReplace() }
                    }
                }
            }
        }
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
            if let graphs = model.graphs {
                Section("Today") {
                    Text("\(graphs.today.answerCount) answers · " +
                         String(format: "%.1f min", Double(graphs.today.answerMillis) / 60000) +
                         " · " + String(format: "%.0f%% correct",
                            graphs.today.answerCount == 0 ? 0 :
                            Double(graphs.today.correctCount) / Double(graphs.today.answerCount) * 100))
                        .font(.callout)
                }
                Section("Cards") {
                    Text("\(graphs.cardCounts.new) new · \(graphs.cardCounts.learn) learn · " +
                         "\(graphs.cardCounts.young) young · \(graphs.cardCounts.mature) mature · " +
                         "\(graphs.cardCounts.suspended) suspended · \(graphs.cardCounts.buried) buried")
                        .font(.caption)
                }
                Section("Reviews/day (30d)") {
                    let byDay = Dictionary(uniqueKeysWithValues: graphs.reviews.map { ($0.day, $0) })
                    SparkBars(values: (-29...0).map { byDay[Int64($0)]?.total ?? 0 },
                              color: .accentColor)
                }
                Section("Added/day (30d)") {
                    let byDay = Dictionary(uniqueKeysWithValues: graphs.added.map { ($0.day, $0.count) })
                    SparkBars(values: (-29...0).map { byDay[Int64($0)] ?? 0 }, color: .orange)
                }
                Section("Future due") {
                    let byDay = Dictionary(uniqueKeysWithValues: graphs.futureDue.map { ($0.day, $0.count) })
                    SparkBars(values: (0..<30).map { byDay[Int64($0)] ?? 0 }, color: .green)
                    Text("daily load \(graphs.dailyLoad)").font(.caption).foregroundStyle(.secondary)
                }
                Section("Intervals") {
                    ForEach(graphs.intervals.prefix(6)) { bucket in
                        HBarRow(label: bucket.label, value: bucket.count,
                                max: graphs.intervals.map(\.count).max() ?? 1, color: .green)
                    }
                }
                Section("Ease") {
                    ForEach(graphs.eases.prefix(5)) { bucket in
                        HBarRow(label: bucket.label, value: bucket.count,
                                max: graphs.eases.map(\.count).max() ?? 1, color: .accentColor)
                    }
                    Text(String(format: "average %.0f%% · retrievability %.1f%% · FSRS %@",
                                graphs.averageEase, graphs.retrievability * 100,
                                graphs.fsrs ? "on" : "off"))
                        .font(.caption).foregroundStyle(.secondary)
                }
                Section("Hourly (all time)") {
                    SparkBars(values: graphs.hours, color: .accentColor)
                }
                Section("Answer buttons") {
                    ForEach(graphs.buttons) { row in
                        HBarRow(label: ["again", "hard", "good", "easy"][max(0, min(3, row.button - 1))],
                                value: row.total,
                                max: graphs.buttons.map(\.total).max() ?? 1,
                                color: .accentColor)
                    }
                }
                Section("Retention") {
                    let young = graphs.retention.youngPassed + graphs.retention.youngFailed
                    let mature = graphs.retention.maturePassed + graphs.retention.matureFailed
                    Text("young \(pct(graphs.retention.youngPassed, young)) (\(young) reps) · " +
                         "mature \(pct(graphs.retention.maturePassed, mature)) (\(mature) reps)")
                        .font(.caption)
                }
            }
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

func pct(_ part: Int, _ total: Int) -> String {
    total == 0 ? "—" : String(format: "%.0f%%", Double(part) / Double(total) * 100)
}

struct SparkBars: View {
    let values: [Int]
    let color: Color

    var body: some View {
        GeometryReader { geo in
            let maxValue = max(1, values.max() ?? 1)
            HStack(alignment: .bottom, spacing: 1) {
                ForEach(values.indices, id: \.self) { index in
                    let height = geo.size.height * CGFloat(values[index]) / CGFloat(maxValue)
                    Rectangle()
                        .fill(values[index] == 0 ? Color.secondary.opacity(0.2) : color)
                        .frame(maxWidth: .infinity)
                        .frame(height: max(0, height))
                }
            }
        }
        .frame(height: 60)
    }
}

struct HBarRow: View {
    let label: String
    let value: Int
    let max maxValue: Int
    let color: Color

    var body: some View {
        HStack(spacing: 6) {
            Text(label).font(.caption2).foregroundStyle(.secondary)
                .frame(width: 56, alignment: .leading)
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 2).fill(Color.secondary.opacity(0.15))
                    RoundedRectangle(cornerRadius: 2).fill(color)
                        .frame(width: maxValue == 0 ? 0 :
                                geo.size.width * CGFloat(value) / CGFloat(maxValue))
                }
            }
            .frame(height: 10)
            Text("\(value)").font(.caption2).foregroundStyle(.secondary)
        }
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
