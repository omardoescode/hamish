<p align="center">
  <img src="packaging/icons/hamish.svg" alt="Hamish logo" width="128" height="128">
</p>

<h1 align="center">Hamish</h1>

<p align="center">
  <em>"Margin" in Arabic.</em> A native desktop writing tool for drafts in progress.<br>
  Not version control, but <strong>alternative control</strong>.
</p>

<p align="center">
  Based on Jason Fried's <a href="https://x.com/jasonfried/status/2105403067793584590">Write_On</a>.
</p>

<p align="center">
  <a href="https://github.com/omardoescode/hamish/releases/latest">Download</a> ·
  <a href="CONTEXT.md">Glossary</a> ·
  <a href="docs/adr/README.md">Decisions</a> ·
  <a href="CODING_STANDARDS.md">Code rules</a>
</p>

---

## What it does

- **Slots:** keep many versions of a word, sentence, or paragraph in place, and switch between them in context.
- **Ghost:** dim text back without deleting it.
- **Overflow:** stash extra text nearby, out of the draft.

One `.md` file per Document. Slots and other state live in a comment at its end, so any Markdown tool still reads the text.

## Install

**Release downloads**

- Linux: unpack the archive and run `./install.sh` (`./install.sh --uninstall` removes it).
- macOS: unzip `Hamish-<target>.app.zip`. The app is unsigned: open it the first time with right-click > Open.

**From source:** `make install`

- Linux: binary, `hamish.desktop` and icons under `~/.local` (set another root with `PREFIX=...`).
- macOS: `~/Applications/Hamish.app`.

Remove it with `make uninstall`.

## Stack

Rust + [GPUI](https://www.gpui.rs/). Run `make help` for the commands.

## Attribution

The idea of Hamish comes from Jason Fried's Write_On, shown in [this post](https://x.com/jasonfried/status/2105403067793584590): alternative control at the word, sentence, and paragraph level, dimmed text, and text stashed nearby.

Status: iteration 1 in progress.
