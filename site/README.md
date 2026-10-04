# Landing page development

Keep the existing source-driven build: version from Cargo.toml, release notes from
CHANGELOG.md, reference content from both READMEs, and demos from the actual binary.
See PROJECT.md for the Linux build prerequisites.

```sh
cargo build --release --locked
python3 site/build.py --binary target/release/command-guardian --out _site
python3 site/check.py --out _site
python3 -m http.server --directory _site
```

The check needs Node.js 18+ and uses the same entry point as Pages. It checks JavaScript,
the language contract, local assets and fragment links; it does not fetch external links.
For visual changes, compare English and Japanese at 1440, 390 and 320 pixels and check
the install CTA, demo tabs and copy button.

`language-preference.js` reads valid `ba0918-language` (`en`/`ja`) first, then the legacy
`cg-lang`, otherwise English. Browser language no longer determines the initial language.
Reads never write or promote legacy settings. Only explicit switches save the shared
key; denied storage still permits in-page switching. Other ba0918 pages read the choice
on navigation/reload, without requiring live synchronization between open tabs.
