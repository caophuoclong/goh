// Developer-only: render the pinned Devicon SVGs. Runtime only embeds the resulting PNGs.
// Run with sharp available on NODE_PATH; no SVG or network support is needed in goh-popup.
const fs = require('node:fs/promises');
const path = require('node:path');
const sharp = require('sharp');
(async () => {
  const root = path.resolve(__dirname, '..');
  const source = path.join(root, 'third_party/devicon');
  for (const file of (await fs.readdir(source)).filter(f => f.endsWith('.svg')).sort()) {
    const name = file.split('-')[0];
    const svg = await fs.readFile(path.join(source, file));
    let pipeline = sharp(svg).resize(64, 64, { fit: 'contain', background: '#00000000' }).ensureAlpha();
    if (name === 'rust' || name === 'github') {
      const {data, info} = await pipeline.raw().toBuffer({ resolveWithObject: true });
      for (let i = 0; i < data.length; i += 4) data[i] = data[i+1] = data[i+2] = 255;
      pipeline = sharp(data, { raw: info });
    }
    await pipeline.png().toFile(path.join(root, 'crates/goh-popup/assets', `${name}.png`));
  }
})();
