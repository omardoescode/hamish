# Ka-teb

Ka-teb (كاتب, "writer") is a desktop writing tool for drafts in progress. It gives the writer alternative control: many candidate texts for one place, switched in context.

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

**Article agreement**:
The rule that changes "a"/"an" before a Word Slot to agree with the Active variant's first sound.
_Avoid_: Article fix

### Markers (UI)

**Marker**:
The visual sign that a Slot has Variants: underline + Variant dots (Word, Sentence) or a left-margin line + vertical dots (Paragraph).

**Variant dots**:
One dot per Variant; the Active variant's dot is filled.

**Bullet point**:
List marker of a Writer Variant.

**Bot point**:
List marker (robot icon) of an AI Variant.

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

**Stash**:
Move (not copy) a selection from the Document to the end of Overflow.
_Avoid_: Cut, park

### Chrome

**Chrome**:
The UI tools around the text: icons, buttons, counter.

**Counter**:
Live word and character count of the Document, or of the selection.

### Lab (not in iteration 1)

**Lab**:
The AI editing popover with Lab checks and Trim levels.

**Lab check**:
An AI action that marks text that matches a rule. It does not change text.

**Trim**:
An AI action that marks words to cut for a target length. It does not rewrite.

**Trim level**:
Original, Slight trim (~10%), Tighten more (~20%), Even sharper (~30%), Cut in half (~50%).

**Trim proposal**:
The set of ranges a Trim marks for cut, with word counts before and after.

**Keep / Make the cuts / Walk through**:
Remove one range from the Trim proposal / apply the proposal / step through each cut.