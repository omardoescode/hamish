# Hamish

Hamish ("margin" in Arabic) is a desktop writing tool for drafts in progress. It gives the writer alternative control: many candidate texts for one place, switched in context.

## Language

### Core idea

**Alternative control**:
More than one candidate text for the same place in a Document; the writer switches between them in context.
_Avoid_: Version control, history

**Document**:
One piece of writing: its text, Slots, Ghost ranges, and Overflow.
_Avoid_: File, note

### Slots and variants

**Slot**:
A place in the Document (word or phrase, sentence, or paragraph) that has two or more Variants.
_Avoid_: Choice, option group

**Variant**:
One candidate text for a Slot.
_Avoid_: Alternative (in code), version, option

**Original**:
The first text written in a Slot. It is the first Variant.

**Active variant**:
The one Variant of a Slot that shows in the Document.
_Avoid_: Selected variant, current version

**Level**:
The granularity of a Slot: Word, Sentence, or Paragraph. Word also covers short phrases.
_Avoid_: Granularity, scope

**Provenance**:
Who wrote a Variant: Writer, AI, or Mixed (an AI Variant the writer edited).
_Avoid_: Author, source

**Tree**:
The Slots and Ghost ranges of one text (the Document, or one Variant), strictly nested (ADR 0006).

**Article agreement**:
The rule that changes "a"/"an" before a Word Slot to agree with the Active variant's first sound.
_Avoid_: Article fix

### Markers (UI)

**Marker**:
The visual sign that a Slot has Variants: underline + Variant dots under a Word or after the end of a Sentence or a left-margin line + vertical dots (Paragraph).

**Variant dots**:
One dot per Variant; the Active variant's dot is filled.

**Bullet point**:
List marker of a Writer Variant.

**Bot point**:
List marker (robot icon) of an AI Variant.

**Alternatives list**:
The rows of the Alternatives panel: the Variants of its target (a Slot, or text with no Slot yet), then an empty row where the writer types a new Variant.
_Avoid_: Variant list

**Quick fill**:
An AI Variants request the writer starts by typing "??" in the empty row of the Alternatives panel. The row dances while the AI works.
_Avoid_: Auto-fill, suggest

**Cycle sound**:
A short tone when the Active variant changes: high for a Variant that is not the Original, low for a return to the Original. The "Cycle sounds" setting turns it, and the Variant delete sound, on or off.
_Avoid_: Beep, click

### Ghost and Overflow

**Ghost**:
A text range the writer has dimmed back. It stays in the Document.
_Avoid_: Hide, dim, comment out

**Revive**:
The command that returns a Ghost range to normal.
_Avoid_: Unghost, restore

**Overflow**:
The right-side panel that holds extra text near the Document, outside the main text.
_Avoid_: Scratchpad, notes, clipboard

**Overflow block**:
A run of lines in Overflow that are not blank. Blank lines split blocks. Using a block puts a copy of its text into the Document.
_Avoid_: Item, chunk

**Stash**:
Move (not copy) a selection from the Document to the end of Overflow.
_Avoid_: Cut, park

### Effects

**Effect**:
A timed visual change: Variant fade-in, Explosion, Undo burst, dance, focus zoom, caret blink. It never blocks input. Its duration comes from one timing source (`Effect` in core).
_Avoid_: Animation (in code), transition

**Reduced motion**:
The setting that makes every Effect instant: no fade, no particles. Off unless the writer turns it on (GPUI does not expose the OS setting).

**Focus zoom**:
The paragraph with the caret shows at 1.02x, a look only (the text keeps its layout). The opposite of Ghost. Off while the Alternatives panel dims the text.
_Avoid_: Magnify, highlight

**Explosion**:
The particle burst that plays at a span (or panel row) when a Variant is deleted.
_Avoid_: Animation, boom

**Undo step**:
One writer action as one undo: a change from any view, or typing (or deleting) in a row at the caret. Undo puts the text and the selection back as they were before it.
_Avoid_: History entry, undo group

**Undo burst**:
The burst that plays at a span when an undo puts a deleted Variant back.

### Markdown

**Markdown style**:
A style the editor draws over Markdown text (heading, bold, italic, inline code, block quote, list marker). The syntax marks stay in the text and stay visible.
_Avoid_: Format, rich text

### Text input

**Caret**:
The insertion point in the Document text or in Overflow, with the selection around it and the text an input method composes. One rule for both texts: a caret clamps to its text before it acts.
_Avoid_: cursor

**Vim mode**:
A setting, off by default. When on, the Editor has a Normal mode and an Insert mode, with Vim keys (ADR 0008).
_Avoid_: Vim emulation, modal editing

**Normal mode**:
The Vim mode state where keys move the Caret and start commands. No key types text.
_Avoid_: Command mode

**Insert mode**:
The Vim mode state where keys type text, as with Vim mode off. Escape goes back to Normal mode.

### Storage

**Hamish comment**:
The HTML comment `<!-- hamish ... -->` at the end of a saved `.md` file. It holds all state except the active text (ADR 0007). It is never Document text.
_Avoid_: Sidecar, metadata

### Chrome

**Chrome**:
The UI tools around the text: icons, buttons, counter.

**Chrome bar**:
The row of Chrome at the window top, with no fill. The text scrolls under it.

**Counter**:
Live word and character count of the Document, or of the selection.

**Shortcuts sheet**:
The sheet that lists the key bindings in groups (Editor, Slots, Overflow, Lab, and Vim mode when it is on). The Shortcuts icon of the Chrome bar opens it; Esc closes it.
_Avoid_: Cheat sheet, keymap help

**Settings**:
The writer's choices for theme, UI scale, UI font, Text font, font size, line height, column width, Reduced motion, Markers, and Markdown styles. They apply at once and save to a JSON file in the XDG config dir.
_Avoid_: Preferences, config

**UI font**:
The font of the Chrome, panels, Settings, menus, and popovers.

**Text font**:
The font of the Document text and the Overflow text. Its default is the UI font.
_Avoid_: Body font

### Lab

**Lab**:
The AI editing popover with Lab checks and Trim levels.

**Lab check**:
An AI action that marks text that matches a rule. It does not change text.

**Lab mark**:
A range a Lab check marks, with an optional note. A "Fix punctuation and typos" mark can carry a proposed fix. Accept puts the fix in place; dismiss removes the mark. A mark moves with text edits and ends when its text is deleted.

**Trim**:
An AI action that marks words to cut for a target length. It does not rewrite.

**Trim level**:
Original, Slight trim (~10%), Tighten more (~20%), Even sharper (~30%), Cut in half (~50%).

**Trim proposal**:
The set of ranges a Trim marks for cut, with word counts before and after.

**Keep / Make the cuts / Walk through**:
Remove one range from the Trim proposal / apply the proposal / step through each cut.