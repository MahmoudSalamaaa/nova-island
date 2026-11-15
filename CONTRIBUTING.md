# Contributing to Nova Island

Thank you for helping. Please read this first, so your work lands in the app people use.

## Where the code is
- **The app is in [`nova-next/`](nova-next/), written in Rust.** It has shipped since 1.8.0.
- **The Electron app is gone.** Files like `nova.js` were removed. A change to Electron code can't
  ship, and a pull request that adds it back will be closed.
- **The root pages are still used.** `index.html`, `renderer.js`, `views/`, `prefs/`, `mini/`,
  `style.css` and `themes.css` are the island, Settings and the mini player, shown in WebView2.
  You can change them, but build `nova-next` afterwards: its `PATCHES` (in
  `nova-next/crates/nova-assets/build.rs`) must still match.

How it fits together, and how to build and test it, is in [nova-next/README.md](nova-next/README.md).

## Before you start
- **Open an issue first** for a new feature or a large change, so we can agree on it before you
  spend time on it.
- **Keep a pull request to one change.** Small pull requests get reviewed sooner.
- **New settings** go in `nova-next/crates/nova-core/src/features.rs`, with a default and its
  check in `pick()`. A setting that isn't there is dropped and never saved.

## Checks
- `cargo build --release -p nova-app` and `cargo test --workspace` pass in `nova-next/`.
- Try your change in the running app: the island closed and open, Settings, and both English and
  Arabic if it shows text.
- Check it with Windows' Text size and display scale above 100%, and with the island vertical.

## Rules
- No new network calls, tracking or analytics without talking about it first. Nova sends nothing
  from people's PCs.
- Don't add dependencies without a good reason; Nova stays small.
- Contributions are under the project's [GPL v3](LICENSE).
