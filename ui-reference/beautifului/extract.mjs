// 从 beautifului.dev 首页 HTML 的 Next.js RSC payload 中提取 21 个组件的源码
import fs from 'fs';

const html = fs.readFileSync('home.html', 'utf8');

// 1. 拼接 flight 流：所有 self.__next_f.push([1,"..."]) 的字符串参数
const chunks = [];
for (const m of html.matchAll(/self\.__next_f\.push\(\[1,("(?:[^"\\]|\\.)*")\]\)/g)) {
  try { chunks.push(JSON.parse(m[1])); } catch {}
}
const flight = chunks.join('');
console.log('flight bytes:', flight.length);

// 2. 解析顶层数据行： id:"string" / id:[...] / id:{...} / id:I...
function parseRow(id) {
  const marker = `${id}:`;
  const i = flight.indexOf(marker);
  if (i < 0) return null;
  let j = i + marker.length;
  const c = flight[j];
  let raw;
  if (c === 'T') {                       // 文本行： id:T<hexlen>,<hexlen 字节原始文本>
    const comma = flight.indexOf(',', j);
    const len = parseInt(flight.slice(j + 1, comma), 16);
    return flight.slice(comma + 1, comma + 1 + len);
  }
  if (c === '"') {                       // 字符串行：扫到未转义引号
    let k = j + 1;
    while (k < flight.length) {
      if (flight[k] === '\\') { k += 2; continue; }
      if (flight[k] === '"') break;
      k++;
    }
    raw = flight.slice(j, k + 1);
    try { return JSON.parse(raw); } catch { return raw; }
  }
  // 非字符串行（数组/对象）：配平括号
  const open = c, close = c === '[' ? ']' : '}';
  let depth = 0, k = j;
  let inStr = false;
  while (k < flight.length) {
    const ch = flight[k];
    if (inStr) { if (ch === '\\') { k += 2; continue; } if (ch === '"') inStr = false; k++; continue; }
    if (ch === '"') inStr = true;
    else if (ch === open) depth++;
    else if (ch === close) { depth--; if (depth === 0) break; }
    k++;
  }
  raw = flight.slice(j, k + 1);
  try { return JSON.parse(raw); } catch { return raw; }
}

// 3. 找 sources 映射
const srcMatch = flight.match(/"sources":\{[^{}]*\}/);
if (!srcMatch) { console.error('sources map not found'); process.exit(1); }
const sources = JSON.parse(srcMatch[0].slice('"sources":'.length));
console.log('components:', Object.keys(sources).length);

// 4. 逐个提取：$12 → 行 12
const outDir = 'components';
fs.mkdirSync(outDir, { recursive: true });
const index = [];
for (const [slug, ref] of Object.entries(sources)) {
  const id = ref.replace('$', '');
  const val = parseRow(id);
  if (val == null) { console.log('MISS', slug, ref); continue; }
  const code = typeof val === 'string' ? val : JSON.stringify(val);
  const file = `${outDir}/${slug}.tsx`;
  fs.writeFileSync(file, code, 'utf8');
  index.push({ slug, bytes: code.length, lines: code.split('\n').length });
}
index.forEach(x => console.log(String(x.bytes).padStart(7), String(x.lines).padStart(4), x.slug));
console.log('total:', index.length);
