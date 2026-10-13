# klepci iOS

Native SwiftUI over the Rust core. **Requires macOS + Xcode** — the sources
here are written but cannot be compiled on Linux.

## One-time setup (on a Mac)

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo install uniffi-bindgen --version 0.32.2
```

## Build the Rust core

```sh
# from the repo root
cargo build -p klep-mobile --release --target aarch64-apple-ios
```

## Generate the Swift bindings

```sh
uniffi-bindgen generate \
  target/aarch64-apple-ios/release/libklep_mobile.a \
  --language swift --out-dir ios/Generated
```

## Xcode project

1. New iOS App project, SwiftUI, bundle id to match your signing.
2. Add all `ios/*.swift` + `ios/Generated/klep_mobile.swift`.
3. Link `libklep_mobile.a` (Build Phases → Link Binary).
4. In `KlepciModel.swift`, replace the stub with the generated `MobileCore`
   (the call sites are marked; API: `MobileCore(path:)`, `decks()`,
   `nextCard()`, `showAnswer(cardId:)`, `answer(cardId:rating:secs:)`,
   `timingJson(search:topN:)`).
5. Run on simulator or device (free Apple ID signing works for personal use;
   App Store distribution needs the paid Developer Program).

Collection path: app documents directory + `/collection.anki2`.
