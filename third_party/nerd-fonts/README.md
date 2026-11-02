Icons from [Nerd Fonts v3.5.1](https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.5.1), the font under the
[MIT license](LICENSE). The popup embeds `crates/goh-popup/assets/nerd-symbols.ttf`: the glyphs of
`crates/goh-theme/src/icons.rs`, cut out of `SymbolsNerdFontMono-Regular.ttf` from `NerdFontsSymbolsOnly.zip`
(SHA-256 `f0f624d9b474bea1662cf7e862d44aebe1ae1f6c7f9cb7a0ca5d0e5ac9561c60`).
Product logos identify the corresponding CLI; no affiliation is implied.

The glyphs come from the icon sets Nerd Fonts collects, under their own licenses:
[Devicons](https://github.com/vorillaz/devicons) and [Seti UI](https://github.com/jesseweed/seti-ui) (MIT),
[Font Awesome](https://github.com/FortAwesome/Font-Awesome) (SIL OFL 1.1),
[Material Design Icons](https://github.com/Templarian/MaterialDesign) (Apache 2.0) and
[Codicons](https://github.com/microsoft/vscode-codicons) (CC BY 4.0).

To regenerate after changing a glyph in `icons.rs` (`pip install fonttools` provides `pyftsubset`):

```sh
scripts/subset-nerd-font.sh SymbolsNerdFontMono-Regular.ttf
```
