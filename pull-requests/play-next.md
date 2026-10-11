## Why

Adding a song to the queue currently puts it after all queued songs. A Play next action lets listeners choose what follows the current song without interrupting it. Refs https://github.com/crmne/spotifast/issues/594.

## What changed

- Add Play next to individual item and selected-song context menus.
- Insert the chosen items at the front of the manual queue, retaining selection order and duplicates. Keep the current track, playback position and remaining context intact.
- Enable the action while the local queue can be reordered, including paused playback. Disable it for other devices; recheck the target when applying the action.
- Update queue documentation, README and translations. Regenerate translation catalogs using the repository script.

## Verification

Passed on Linux (Debian 13), with default and all features:

```sh
python3 packaging/test-launchers.py
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --all-targets --all-features
cargo test --locked --all-features --doc
RUSTDOCFLAGS='-D warnings' cargo doc --locked --all-features --no-deps
(cd docs && bundle exec jekyll build)
.github/scripts/update-translations.sh --check
```

Also passed `python3 packaging/test-release-names.py` and `python3 packaging/flatpak/test-metainfo.py`.

Regression tests cover insertion order, duplicate items, existing and empty manual queues, playing and paused state, backend updates, rapid clicks, and a playback-target change before applying the action. Demo tests exercise single and multiple selection menus and the disabled action for another device.

Actual native demo captures compare upstream `dd2d5e3` with `feat/play-next` across light/dark themes, normal/narrow windows and single/multiple/remote menu states. See `reviews/play-next.html` in the linked visual review download. These were captured on Linux with X11/Xvfb and software OpenGL. macOS, Windows and live Spotify playback were not exercised here.

## Visual review

[Download the complete HTML comparison and all captures](https://github.com/nikankad/spotifast/archive/refs/heads/pr-review-evidence.zip), extract the ZIP, then open `reviews/index.html`. [Review source and validation results](https://github.com/nikankad/spotifast/tree/pr-review-evidence).

| Window and theme | Before | After |
| --- | --- | --- |
| Normal, dark | ![Before](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/play-next/dark-normal-single-before.png) | ![After](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/play-next/dark-normal-single-after.png) |
| Narrow, light | ![Before](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/play-next/light-narrow-single-before.png) | ![After](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/play-next/light-narrow-single-after.png) |

- [x] I read `CONTRIBUTING.md` and kept this pull request to one concern.
- [x] I added or updated tests for behaviour that can regress.
- [x] I ran formatting, Clippy, and the relevant tests locally.
- [x] I updated documentation for user-visible behaviour, settings, files, or network access.
- [ ] I understand and take responsibility for every submitted line, including AI-assisted code.
