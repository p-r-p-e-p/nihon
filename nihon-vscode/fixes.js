// Lógica das correções, sem dependência do `vscode` (testável com `node`).

// `col` do nihonc: base 1, conta caracteres (code points).
// O VS Code conta unidades UTF-16 a partir de 0. Só difere com kanji raros (planos astrais).
function toUtf16Column(lineText, col) {
  let units = 0;
  let n = 0;
  for (const ch of lineText) {
    if (n === col - 1) break;
    units += ch.length;
    n++;
  }
  return units;
}

// "use the canonical form" -> "Use the canonical form: `関数`"
function fixTitle(message, replacement) {
  const t = message.charAt(0).toUpperCase() + message.slice(1);
  const r = replacement.trim();
  if (r && !message.includes(r) && !r.includes('\n') && [...r].length <= 24) return `${t}: \`${r}\``;
  return t;
}

function cmp(a, b) {
  return a.line - b.line || a.ch - b.ch;
}

// Escolhe as edições que podem ir juntas: sem sobreposição e sem repetidas.
// Cada item: { range: { start: {line, ch}, end: {line, ch} }, replacement }
function pickNonOverlapping(items) {
  const sorted = [...items].sort((a, b) => cmp(a.range.start, b.range.start) || cmp(a.range.end, b.range.end));
  const out = [];
  const seen = new Set();
  let lastEnd = null;
  for (const it of sorted) {
    const key = `${it.range.start.line}:${it.range.start.ch}-${it.range.end.line}:${it.range.end.ch}=${it.replacement}`;
    if (seen.has(key)) continue;
    if (lastEnd && cmp(it.range.start, lastEnd) < 0) continue;
    seen.add(key);
    out.push(it);
    lastEnd = it.range.end;
  }
  return out;
}

module.exports = { toUtf16Column, fixTitle, pickNonOverlapping };
