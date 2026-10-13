import SwiftUI

// Models mirror the UniFFI-generated `MobileCore` API
// (klep_mobile.swift, generated — see ios/README.md).
// Collection lives in the app sandbox: documents/collection.anki2.

@main
struct KlepciApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .preferredColorScheme(.dark)
        }
    }
}

struct ContentView: View {
    @StateObject private var model = KlepciModel()
    @State private var tab = 0

    var body: some View {
        TabView(selection: $tab) {
            DecksView(model: model, goReview: { tab = 1 })
                .tabItem { Label("Decks", systemImage: "tray") }.tag(0)
            ReviewView(model: model)
                .tabItem { Label("Review", systemImage: "brain") }.tag(1)
            BrowseView(model: model)
                .tabItem { Label("Browse", systemImage: "magnifyingglass") }.tag(2)
            StatsView(model: model)
                .tabItem { Label("Stats", systemImage: "chart.bar") }.tag(3)
        }
        .alert("Error", isPresented: Binding(
            get: { model.error != nil },
            set: { if !$0 { model.error = nil } }
        )) {
            Button("OK") { model.error = nil }
        } message: {
            Text(model.error ?? "")
        }
    }
}
