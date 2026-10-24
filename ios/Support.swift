import Foundation

// NOTE: `import klep_mobile` (the UniFFI-generated Swift module) is required.
// Build it per ios/README.md, then uncomment the import and remove this shim.
// Until then this file type-checks the app logic against local protocols.

func htmlToText(_ html: String) -> String {
    var out = html
        .replacingOccurrences(of: "<br>", with: "\n")
        .replacingOccurrences(of: "<br/>", with: "\n")
        .replacingOccurrences(of: "</p>", with: "\n")
        .replacingOccurrences(of: "</div>", with: "\n")
        .replacingOccurrences(of: "</li>", with: "\n")
        .replacingOccurrences(of: "<li>", with: "\n• ")
    while let s = out.range(of: "[sound:"),
          let e = out.range(of: "]", range: s.upperBound..<out.endIndex) {
        out.replaceSubrange(s.lowerBound..<e.upperBound, with: "🔊")
    }
    while let s = out.range(of: "<img"),
          let e = out.range(of: ">", range: s.upperBound..<out.endIndex) {
        out.replaceSubrange(s.lowerBound..<e.upperBound, with: "[image]")
    }
    var text = ""
    var inside = false
    for ch in out {
        if ch == "<" { inside = true } else if ch == ">" { inside = false }
        else if !inside { text.append(ch) }
    }
    return text
        .replacingOccurrences(of: "&nbsp;", with: " ")
        .replacingOccurrences(of: "&lt;", with: "<")
        .replacingOccurrences(of: "&gt;", with: ">")
        .replacingOccurrences(of: "&amp;", with: "&")
        .replacingOccurrences(of: "&quot;", with: "\"")
        .split(separator: "\n", omittingEmptySubsequences: false)
        .map { $0.replacingOccurrences(of: "\\s+$", with: "", options: .regularExpression) }
        .joined(separator: "\n")
        .trimmingCharacters(in: .whitespacesAndNewlines)
}

func formatSecs(_ s: Double) -> String {
    s < 60 ? String(format: "%.1fs", s) : String(format: "%.1fm", s / 60)
}

struct DeckRow: Identifiable {
    let id: Int64
    let name: String
    var level: Int = 1
    var newCount: Int = 0
    var learnCount: Int = 0
    var reviewCount: Int = 0
    var total: Int = 0
    var filtered = false
}

struct DeckOptionsRow {
    var deckId: Int64
    var presetId: Int64
    var presetName: String
    var newPerDay: Int
    var reviewsPerDay: Int
    var learnSteps: String
    var relearnSteps: String
    var graduatingIntervalGood: Int
    var graduatingIntervalEasy: Int
    var easyMultiplier: Double
    var hardMultiplier: Double
    var intervalMultiplier: Double
    var maximumReviewInterval: Int
    var minimumLapseInterval: Int
    var leechThreshold: Int
    var leechAction: Int
    var desiredRetention: Double
    var fsrs: Bool
    var buryNew: Bool
    var buryReviews: Bool
    var buryInterdayLearning: Bool
    var showTimer: Bool
    var capAnswerTimeToSecs: Int
}

struct DayReviewsRow: Identifiable {
    var id: Int64 { day }
    let day: Int64
    let learn: Int
    let relearn: Int
    let young: Int
    let mature: Int
    let filtered: Int
    var total: Int { learn + relearn + young + mature + filtered }
}

struct DayCountRow: Identifiable {
    var id: Int64 { day }
    let day: Int64
    let count: Int
}

struct BucketRow: Identifiable {
    var id: Int64 { key }
    let key: Int64
    let label: String
    let count: Int
}

struct ButtonRowData: Identifiable {
    var id: Int64 { button }
    let button: Int
    let learning: Int
    let young: Int
    let mature: Int
    var total: Int { learning + young + mature }
}

struct CardCountsRow {
    var new = 0
    var learn = 0
    var relearn = 0
    var young = 0
    var mature = 0
    var suspended = 0
    var buried = 0
}

struct TodayRow {
    var answerCount = 0
    var answerMillis = 0
    var correctCount = 0
    var learnCount = 0
    var reviewCount = 0
    var relearnCount = 0
}

struct RetentionRow {
    var youngPassed = 0
    var youngFailed = 0
    var maturePassed = 0
    var matureFailed = 0
}

struct GraphsRow {
    var reviews: [DayReviewsRow] = []
    var added: [DayCountRow] = []
    var intervals: [BucketRow] = []
    var eases: [BucketRow] = []
    var hours: [Int] = []
    var futureDue: [DayCountRow] = []
    var cardCounts = CardCountsRow()
    var today = TodayRow()
    var buttons: [ButtonRowData] = []
    var retention = RetentionRow()
    var averageEase: Double = 0
    var retrievability: Double = 0
    var dailyLoad = 0
    var fsrs = false
}

struct SlowCard: Identifiable {
    let id: Int64
    let reps: Int
    let mean: Double
    let worst: Double
}
