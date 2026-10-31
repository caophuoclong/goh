# macOS packaging

`Goh.app` is the bundle around `goh-popup` (scripts/macos-popup-app.sh): its name is what System Settings >
Privacy & Security > Accessibility and macOS's permission prompt show.

- `Goh.Info.plist`: the bundle's Info.plist. The identifier `io.github.caophuoclong.goh.popup` must never change:
  macOS keeps the Accessibility permission under it (and the signing certificate). The name and icon may change.
- The icon: `Goh.png` (1024×1024, square, transparent background; macOS does not round it for you), or a
  ready `Goh.icns`. The next build makes `Contents/Resources/Goh.icns` from it. `Goh.svg` is its source, on
  macOS's icon grid (an 824 body in the 1024 canvas); change the SVG, then export it again to `Goh.png`. Without either, macOS shows its
  generic app icon. System Settings caches icons: an entry added before the icon changed may keep the old one until
  it is removed (−) and allowed again.
