# Hamish

Hamish ("writer") is a native desktop writing tool for drafts in progress. Keep many versions of a word, sentence, or paragraph in place and switch between them. Dim text back (Ghost) and stash extra text nearby (Overflow).

Stack: Rust + GPUI. Storage: one `.md` file; Slots and other state in a comment at its end.

- Glossary: [CONTEXT.md](CONTEXT.md)
- Decisions: [docs/adr/](docs/adr/README.md)
- Code rules: [CODING_STANDARDS.md](CODING_STANDARDS.md)

Install as a desktop app: `make install` (Linux: binary, `hamish.desktop` and icons under `~/.local`, set another root with `PREFIX=...`; macOS: `~/Applications/Hamish.app`). Remove it with `make uninstall`.

Release downloads: on Linux, unpack the archive and run `./install.sh` (`./install.sh --uninstall` removes it). On macOS, unzip `Hamish-<target>.app.zip`. The app is unsigned: open it the first time with right-click > Open.

Status: iteration 1 in progress.

## Attribution

The idea of Hamish comes from Jason Fried's Write_On, shown in [this post](https://x.com/jasonfried/status/2105403067793584590): alternative control at the word, sentence, and paragraph level, dimmed text, and text stashed nearby. "Not version control, but alternative control."
