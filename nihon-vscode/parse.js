// Lê a saída do nihonc (formato rustc) e devolve diagnósticos estruturados.
// Sem dependência do `vscode`, para poder ser testado com `node`.

const HEADER = /^(error|warning)\[(\w+)\]: (.*)$/;
const LOCATION = /^\s*--> (.+):(\d+):(\d+)\s*$/;
const EXTRA = /^(?:\s*= )?(note|help): (.*)$/;

function parseOutput(text) {
  const lines = text.split(/\r?\n/);
  const out = [];
  let cur = null;
  for (const line of lines) {
    const h = HEADER.exec(line);
    if (h) {
      cur = { severity: h[1], code: h[2], message: h[3], line: null, col: null, extras: [] };
      out.push(cur);
      continue;
    }
    if (!cur) continue;
    const loc = LOCATION.exec(line);
    if (loc && cur.line === null) {
      cur.line = Number(loc[2]);
      cur.col = Number(loc[3]);
      continue;
    }
    const ex = EXTRA.exec(line);
    if (ex) cur.extras.push(`${ex[1]}: ${ex[2]}`);
  }
  return out.filter((d) => d.line !== null);
}

module.exports = { parseOutput };
