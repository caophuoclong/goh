Command logos from [Devicon v2.17.0](https://github.com/devicons/devicon/tree/v2.17.0), distributed under
the [MIT license](LICENSE). `sources.json` records source paths and SHA-256 hashes. Product logos identify
the corresponding CLI; no affiliation is implied.

Original SVGs are retained here. The popup embeds the 64×64 transparent RGBA PNG derivatives in
`crates/goh-popup/assets/`, so it needs no font, SVG renderer, network access or new runtime dependency.
Rust and GitHub monochrome alpha masks are tinted by the popup theme for contrast.

To regenerate, make `sharp` available on Node's module path and run from the repository:

```sh
NODE_PATH=/path/to/node_modules node tools/render-command-icons.cjs
```
