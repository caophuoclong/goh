One suggestion: a kind glyph, the label, and an optional one-line description.

Provide `label` and, if known, `description` and `kind` (`subcommand`, `option`, `path`, `argument`). Pass `matches`, a list of `[start, end)` ranges in code points of the label, to draw those characters in `text-match`, bold and underlined. Each result carries its own ranges, because fuzzy matches are not always a prefix; the adapter snaps them to grapheme clusters so combining marks are never split. The selected row shows `❯` in `accent`, `surface-selected` and weight 600.

Use it only inside `SuggestionPopup`. The description shrinks and truncates before the label does; the label's full text is its `title`. Do not show icons or emoji in place of the glyph.
