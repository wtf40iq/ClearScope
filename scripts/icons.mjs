// Renders the app icons from the SVG sources.
//   src-tauri/icons/icon.svg  – full artwork (48 px and up)
//   public/logo.svg           – simplified artwork that stays readable at 16–32 px
// Run: node scripts/icons.mjs
import sharp from "sharp";
import { readFileSync, writeFileSync } from "node:fs";

const full = readFileSync("src-tauri/icons/icon.svg");
const small = readFileSync("public/logo.svg");
const png = (svg, size) => sharp(svg, { density: 72 * Math.max(1, size / 256) * 4 }).resize(size, size).png({ compressionLevel: 9 }).toBuffer();

const out = "src-tauri/icons/";
writeFileSync(out + "32x32.png", await png(small, 32));
writeFileSync(out + "128x128.png", await png(full, 128));
writeFileSync(out + "128x128@2x.png", await png(full, 256));
writeFileSync(out + "icon.png", await png(full, 512));

// classic .ico: 32-bit BMP entries, PNG only for 256 px (what NSIS and old shells expect)
async function bmp(svg, size) {
  const rgba = await sharp(await png(svg, size)).ensureAlpha().raw().toBuffer();
  const maskRow = Math.ceil(size / 32) * 4;
  const hdr = Buffer.alloc(40);
  hdr.writeUInt32LE(40, 0); hdr.writeInt32LE(size, 4); hdr.writeInt32LE(size * 2, 8);
  hdr.writeUInt16LE(1, 12); hdr.writeUInt16LE(32, 14);
  hdr.writeUInt32LE(size * size * 4 + maskRow * size, 20);
  const px = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
    const s = (y * size + x) * 4, d = ((size - 1 - y) * size + x) * 4;
    px[d] = rgba[s + 2]; px[d + 1] = rgba[s + 1]; px[d + 2] = rgba[s]; px[d + 3] = rgba[s + 3];
  }
  return Buffer.concat([hdr, px, Buffer.alloc(maskRow * size)]);
}
const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
const imgs = await Promise.all(sizes.map((s) => {
  const svg = s <= 32 ? small : full;
  return s >= 256 ? png(svg, s) : bmp(svg, s);
}));
const head = Buffer.alloc(6 + 16 * sizes.length);
head.writeUInt16LE(0, 0); head.writeUInt16LE(1, 2); head.writeUInt16LE(sizes.length, 4);
let off = head.length;
sizes.forEach((s, i) => {
  const e = 6 + 16 * i;
  head.writeUInt8(s >= 256 ? 0 : s, e); head.writeUInt8(s >= 256 ? 0 : s, e + 1);
  head.writeUInt16LE(1, e + 4); head.writeUInt16LE(32, e + 6);
  head.writeUInt32LE(imgs[i].length, e + 8); head.writeUInt32LE(off, e + 12);
  off += imgs[i].length;
});
writeFileSync(out + "icon.ico", Buffer.concat([head, ...imgs]));
console.log("icons written to", out);
