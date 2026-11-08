// Lê a saída do nihonc e devolve diagnósticos num formato único.
// Sem dependência do `vscode`, para poder ser testado com `node`.
//
// Formato devolvido:
//   { severity, code, message, extras: [texto...],
//     start: {line, col}, end: {line, col} | null,      (base 1, col em caracteres)
//     fixes: [{ message, start, end, replacement, safe }] }

const HEADER = /^(error|warning)\[(\w+)\]: (.*)$/;
const LOCATION = /^\s*--> (.+):(\d+):(\d+)\s*$/;
const EXTRA = /^(?:\s*= )?(note|help): (.*)$/;

// Saída normal (humana). Não traz as correções de forma estruturada, então
// aqui `fixes` fica sempre vazio: quick fix precisa de --message-format=json.
function parseOutput(text) {
  const out = [];
  let cur = null;
  for (const line of text.split(/\r?\n/)) {
    const h = HEADER.exec(line);
    if (h) {
      cur = {
        severity: h[1], code: h[2], message: h[3], extras: [],
        start: { line: 1, col: 1 }, end: null, fixes: [], located: false,
      };
      out.push(cur);
      continue;
    }
    if (!cur) continue;
    const loc = LOCATION.exec(line);
    if (loc && !cur.located) {
      cur.start = { line: Number(loc[2]), col: Number(loc[3]) };
      cur.located = true;
      continue;
    }
    const ex = EXTRA.exec(line);
    if (ex) cur.extras.push(`${ex[1]}: ${ex[2]}`);
  }
  return out.map(({ located, ...d }) => d);
}

function pos(p) {
  return p && Number.isInteger(p.line) && Number.isInteger(p.col) ? { line: p.line, col: p.col } : null;
}

// `--message-format=json`: um objeto por linha.
function parseJson(text) {
  const out = [];
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line.startsWith('{')) continue;
    let d;
    try {
      d = JSON.parse(line);
    } catch {
      continue;
    }
    if (!d || (d.level !== 'error' && d.level !== 'warning')) continue;

    const spans = Array.isArray(d.spans) ? d.spans : [];
    const sp = spans.find((s) => s.primary) || spans[0];
    const extras = [];
    if (sp && sp.label && sp.label !== d.message) extras.push(String(sp.label));
    for (const n of Array.isArray(d.notes) ? d.notes : []) extras.push(`note: ${n}`);

    const fixes = [];
    for (const h of Array.isArray(d.helps) ? d.helps : []) {
      const e = h && h.edit;
      const s = e && pos(e.start);
      const en = e && pos(e.end);
      if (s && en) {
        fixes.push({
          message: String(h.message || ''),
          start: s,
          end: en,
          replacement: typeof e.replacement === 'string' ? e.replacement : '',
          safe: e.safe === true,
        });
      } else if (h && h.message) {
        extras.push(`help: ${h.message}`);
      }
    }

    out.push({
      severity: d.level,
      code: String(d.code || ''),
      message: String(d.message || ''),
      extras,
      start: (sp && pos(sp.start)) || { line: 1, col: 1 },
      end: (sp && pos(sp.end)) || null,
      fixes,
    });
  }
  return out;
}

module.exports = { parseOutput, parseJson };
