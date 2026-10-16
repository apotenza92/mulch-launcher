// Run: npm install --prefix target/icon-tools --no-save @resvg/resvg-js
//      node scripts/generate-icons.cjs
// Render each size from the vector source, never upscale a smaller PNG.
const fs = require('node:fs');
const path = require('node:path');
const { Resvg } = require('../target/icon-tools/node_modules/@resvg/resvg-js');
const root = path.resolve(__dirname, '..');
const brand = path.join(root, 'assets/brand');
const assets = path.join(root, 'packaging/store/Assets');
const listing = path.join(root, 'packaging/store/listing');
const source = fs.readFileSync(path.join(brand, 'logo.svg'), 'utf8');
// The tile's corner radius is 10/240, the 2px at 48px that Windows app icons use.
// One pixel of source-space clearance preserves antialiasing at the tile edge.
const tight = source.replace('viewBox="0 0 256 256"', 'viewBox="7 7 242 242"');
function render(svg, size) {
  return new Resvg(svg, { fitTo: { mode: 'width', value: size } }).render().asPng();
}
function write(name, size, svg = tight) {
  fs.writeFileSync(path.join(assets, name), render(svg, size));
}
for (const scale of [100, 125, 150, 200, 250, 300, 400]) {
  for (const [name, base] of [['Square44x44Logo', 44], ['Square150x150Logo', 150], ['StoreLogo', 50]]) {
    write(`${name}.scale-${scale}.png`, Math.round(base * scale / 100));
  }
  const width = Math.round(310 * scale / 100);
  const height = Math.round(150 * scale / 100);
  // Keep the square artwork proportional in the centre of the wide tile.
  const inner = tight.replace('<svg ', `<svg x="${(width - height) / 2}" y="0" width="${height}" height="${height}" `);
  const wide = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">${inner}</svg>`;
  write(`Wide310x150Logo.scale-${scale}.png`, width, wide);
}
const sizes = [16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256];
for (const size of sizes) {
  for (const suffix of ['', '_altform-unplated', '_altform-lightunplated']) {
    write(`Square44x44Logo.targetsize-${size}${suffix}.png`, size);
  }
}
for (const size of [71, 150, 300]) {
  fs.writeFileSync(path.join(listing, `app-tile-${size}.png`), render(tight, size));
}
// The README logo keeps the source's margin.
fs.writeFileSync(path.join(brand, 'logo-256.png'), render(source, 256));
// ICO entries can contain lossless PNGs. Include every Windows target size.
const frames = sizes.map(size => ({ size, data: render(tight, size) }));
const header = Buffer.alloc(6 + frames.length * 16);
header.writeUInt16LE(1, 2);
header.writeUInt16LE(frames.length, 4);
let offset = header.length;
frames.forEach(({size, data}, index) => {
  const at = 6 + index * 16;
  header[at] = header[at + 1] = size === 256 ? 0 : size;
  header.writeUInt16LE(1, at + 4);
  header.writeUInt16LE(32, at + 6);
  header.writeUInt32LE(data.length, at + 8);
  header.writeUInt32LE(offset, at + 12);
  offset += data.length;
});
fs.writeFileSync(path.join(brand, 'MulchLauncher.ico'), Buffer.concat([header, ...frames.map(frame => frame.data)]));
console.log('Generated Store listing, scaled package, theme variants and ICO assets and the README logo from logo.svg.');
