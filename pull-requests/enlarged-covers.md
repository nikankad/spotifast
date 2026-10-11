## Why

Album and playlist artwork is currently confined to the small cover in the page header. Clicking the cover should let listeners inspect it at a larger size. Closes https://github.com/crmne/spotifast/issues/586.

## What changed

- Make album and playlist covers open an enlarged artwork dialog, with an accessible label, hover hint and keyboard focus indicator.
- Fit the image to the available window while preserving its aspect ratio. Show a compact loading indicator and contain failed-image content within the viewer.
- Support Close, Escape and backdrop dismissal without changing the page or playback.
- Keep missing artwork and decorative covers without the enlargement action.
- Update the user guide, README and translations. Regenerate translation catalogs using the repository script.

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

Regression tests cover album and playlist opening, all dismissal paths, preserved page and playback, missing artwork, non-square image sizing, light/dark themes, small windows, and decode errors.

Actual native demo captures compare upstream `dd2d5e3` with `feat/enlarged-covers` across light/dark themes, normal/narrow windows, album and playlist pages, loading and error states, and the decorative Liked Songs cover. See `reviews/enlarged-covers.html` in the linked visual review download. These were captured on Linux with X11/Xvfb and software OpenGL. macOS and Windows were not exercised here.

## Visual review

[Download the complete HTML comparison and all captures](https://github.com/nikankad/spotifast/archive/refs/heads/pr-review-evidence.zip), extract the ZIP, then open `reviews/index.html`. [Review source and validation results](https://github.com/nikankad/spotifast/tree/pr-review-evidence).

| Window and theme | Before | After |
| --- | --- | --- |
| Normal, dark | ![Before](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/enlarged-covers/dark-normal-album-before.png) | ![After](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/enlarged-covers/dark-normal-album-after.png) |
| Narrow, light | ![Before](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/enlarged-covers/light-narrow-album-before.png) | ![After](https://raw.githubusercontent.com/nikankad/spotifast/pr-review-evidence/reviews/enlarged-covers/light-narrow-album-after.png) |

- [x] I read `CONTRIBUTING.md` and kept this pull request to one concern.
- [x] I added or updated tests for behaviour that can regress.
- [x] I ran formatting, Clippy, and the relevant tests locally.
- [x] I updated documentation for user-visible behaviour, settings, files, or network access.
- [ ] I understand and take responsibility for every submitted line, including AI-assisted code.
