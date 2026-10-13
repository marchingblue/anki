# klepci Android

Native Kotlin + Jetpack Compose UI over the Rust core (`klep-mobile`
UniFFI bindings). Same three screens as desktop: Decks, Review, Stats.

## One-time setup

```sh
# Rust targets (needs the NDK — already installed via sdkmanager here)
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
cargo install cargo-ndk
# UniFFI bindings generator (must match the `uniffi` crate version)
cargo install uniffi-bindgen --version 0.32.2
```

## Build the Rust core

```sh
# from the repo root
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
  -o android/app/src/main/jniLibs build -p klep-mobile --release
```

## Generate the Kotlin bindings

```sh
uniffi-bindgen generate --lib \
  target/aarch64-linux-android/release/libklep_mobile.so \
  --language kotlin \
  --out-dir android/app/src/main/java
```

## Build the APK

```sh
cd android
./gradlew assembleDebug   # needs Gradle wrapper; first run downloads it
```

The collection lives at the app's `filesDir/collection.anki2`. To seed it,
copy a `collection.anki2` (+ `collection.media`) from desktop before first
launch, or implement `.apkg` import later (planned).
