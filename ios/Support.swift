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

struct SlowCard: Identifiable {
    let id: Int64
    let reps: Int
    let mean: Double
    let worst: Double
}
