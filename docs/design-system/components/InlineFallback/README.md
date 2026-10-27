The top suggestion as ghost text on the prompt line, with the remaining count and the `Tab` hint at the right.

The shell adapter, not this component, decides what Tab would insert. Provide `before` (line text before the cursor), `insertion` (exactly the text the adapter will insert at the cursor, already quoted or escaped for the shell) and `after` (line text after the cursor, shown unchanged so completion in the middle of a line is visible). Do not derive the insertion from the last word: quoted paths and cursor-in-the-middle cases belong to the adapter. If the adapter has no valid insertion, render nothing.

Use it on Wayland, in tmux, over SSH, below `popup-width-min`, and anywhere a caret position is unavailable. It is flat: no shadow. The line truncates at the end with an ellipsis; the hint stays visible.
