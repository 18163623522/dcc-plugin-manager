// 最终版：T 行 hexlen = 解码文本的 UTF-8 字节长度。按字节截取再解码。
import fs from 'fs';

const html = fs.readFileSync('home.html', 'utf8');
const parts = [];
for (const m of html.matchAll(/self\.__next_f\.push\(\[1,("(?:[^"\\]|\\.)*")\]\)/g)) {
  parts.push(m[1].slice(1, -1));
}
const flight = parts.map(p => { try { return JSON.parse('"' + p + '"'); } catch { return ''; } }).join('');
const sources = JSON.parse(flight.match(/"sources":\{[^{}]*\}/)[0].slice('"sources":'.length));

const enc = new TextEncoder();
const dec = new TextDecoder();

function extract(id) {
  const re = new RegExp(`(?:^|\\n)${id}:T([0-9a-f]+),`, 'g');
  let m, hit = null;
  while ((m = re.exec(flight))) {
    if (hit) return { error: 'ambiguous' };
    hit = m;
  }
  if (!hit) return { error: 'notfound' };
  const start = hit.index + hit[0].length;
  const hexlen = parseInt(hit[1], 16);
  // 候选区（富余），按 UTF-8 字节精确截到 hexlen
  const candidate = flight.slice(start, start + hexlen + 4096);
  let buf = enc.encode(candidate);
  if (buf.length < hexlen) return { error: 'shortstream' };
  let cut = hexlen;
  // 防止把多字节字符切半
  while (cut > 0 && (buf[cut] & 0xC0) === 0x80) cut--;
  const text = dec.decode(buf.subarray(0, cut));
  const exact = enc.encode(text).length === hexlen;
  return { text, hexlen, exact };
}

const out = "components";
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out);
let bad = 0;
for (const [slug, ref] of Object.entries(sources)) {
  const r = extract(ref.slice(1));
  if (r.error) { console.log('FAIL', slug, r.error); bad++; continue; }
  fs.writeFileSync(`${out}/${slug}.tsx`, r.text, 'utf8');
  const ok = r.exact && r.text.startsWith('"use client"') && /export /.test(r.text) && r.text.trimEnd().endsWith('}');
  if (!ok) bad++;
  console.log(ok ? 'OK ' : 'BAD', String(r.hexlen).padStart(6), 'bytesExact=' + r.exact, slug,
    !ok ? 'end=' + JSON.stringify(r.text.slice(-40)) : '');
}
console.log(bad === 0 ? 'ALL 21 CLEAN ✓' : bad + ' bad');
