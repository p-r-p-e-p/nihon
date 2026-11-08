// Teste de ponta a ponta: extensão (com um `vscode` de mentira) + nihonc de verdade.
// Aplica os quick fixes no texto, recompila, e confere que os problemas somem.
// Uso: NIHONC=/caminho/nihonc node test/e2e.js   (pula se o binário não existir)
const assert = require('assert');
const fs = require('fs');
const os = require('os');
const path = require('path');
const cp = require('child_process');
const Module = require('module');

const NIHONC = process.env.NIHONC || '/home/claude/nihonc/target/release/nihonc';
if (!fs.existsSync(NIHONC)) {
  console.log('e2e.js: nihonc nao encontrado, pulando');
  process.exit(0);
}

// ------------------------------------------------------------ vscode de mentira
class Position {
  constructor(l, c) { this.line = l; this.character = c; }
  translate(dl, dc) { return new Position(this.line + dl, this.character + dc); }
}
class Range {
  constructor(a, b, c, d) {
    if (a instanceof Position) { this.start = a; this.end = b; }
    else { this.start = new Position(a, b); this.end = new Position(c, d); }
  }
  get isEmpty() { return this.start.line === this.end.line && this.start.character === this.end.character; }
}
class Kind {
  constructor(v) { this.value = v; }
  append(p) { return new Kind(`${this.value}.${p}`); }
  contains(o) { return o.value === this.value || o.value.startsWith(`${this.value}.`); }
  intersects(o) { return this.contains(o) || o.contains(this); }
}
Kind.QuickFix = new Kind('quickfix');
Kind.SourceFixAll = new Kind('source.fixAll');

const settings = { compilerPath: NIHONC, 'diagnostics.enabled': true, 'diagnostics.run': 'onSave', strict: false, romaji: false };
const collections = new Map();
const registered = {};
const noop = () => ({ dispose() {} });
const vscode = {
  Position, Range,
  Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
  DiagnosticSeverity: { Error: 0, Warning: 1 },
  CodeAction: class { constructor(title, kind) { this.title = title; this.kind = kind; } },
  CodeActionKind: Kind,
  WorkspaceEdit: class { constructor() { this.edits = []; } replace(uri, range, text) { this.edits.push({ range, text }); } },
  MarkdownString: class { appendMarkdown() {} },
  Hover: class {}, CompletionItem: class { constructor(l, k) { this.label = l; this.kind = k; } },
  CompletionItemKind: { Keyword: 0, Function: 1, Class: 2 }, ViewColumn: { Beside: 2 },
  languages: {
    createDiagnosticCollection: () => {
      const m = new Map();
      collections.set('nihon', m);
      return { set: (u, v) => m.set(u.toString(), v), get: (u) => m.get(u.toString()), delete: (u) => m.delete(u.toString()), dispose() {} };
    },
    registerCodeActionsProvider: (s, p) => { registered.codeActions = p; return noop(); },
    registerHoverProvider: noop, registerCompletionItemProvider: noop,
  },
  workspace: {
    getConfiguration: () => ({ get: (k) => settings[k] }),
    getWorkspaceFolder: () => undefined,
    onDidSaveTextDocument: noop, onDidOpenTextDocument: noop, onDidChangeTextDocument: noop,
    onDidCloseTextDocument: noop, onDidChangeConfiguration: noop, textDocuments: [],
  },
  window: { showWarningMessage: (m) => { throw new Error(`aviso inesperado: ${m}`); }, showInformationMessage: noop },
  commands: { registerCommand: noop },
};
const origLoad = Module._load;
Module._load = function (req, ...a) { return req === 'vscode' ? vscode : origLoad.call(this, req, ...a); };
const ext = require('../extension');

// ------------------------------------------------------------ utilitários
const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'nihon-e2e-'));
function makeDoc(name, text) {
  const file = path.join(dir, name);
  fs.writeFileSync(file, text);
  const lines = text.split('\n');
  return {
    languageId: 'nihon', fileName: file, isDirty: false,
    uri: { scheme: 'file', toString: () => `file://${file}` },
    get lineCount() { return lines.length },
    lineAt: (n) => ({ text: lines[n] }),
    getText: () => text,
    getWordRangeAtPosition: () => undefined,
  };
}
function apply(text, edits) {
  const lines = text.split('\n');
  const off = (p) => lines.slice(0, p.line).reduce((n, l) => n + l.length + 1, 0) + p.character;
  let out = text;
  for (const e of [...edits].sort((a, b) => off(b.range.start) - off(a.range.start))) {
    out = out.slice(0, off(e.range.start)) + e.text + out.slice(off(e.range.end));
  }
  return out;
}
function nihonc(file, ...flags) {
  return cp.spawnSync(NIHONC, ['--check', ...flags, file], { encoding: 'utf8' });
}
const diagsOf = (doc) => collections.get('nihon').get(doc.uri.toString()) || [];

(async () => {
  ext.activate({ subscriptions: [] });
  const provider = registered.codeActions;
  assert.ok(provider, 'provider de code actions nao registrado');

  // ---------- 1. posições dos diagnósticos
  let text = [
    'kansuu 主() {',
    '    束縛 x = １；',
    '    もしx > 0 { 表示!("a"); }',
    '    束縛 可変 z = 3;',
    '    束縛 ﾊﾝｶｸ = 4;',
    '    表示!("{} {}", z, ﾊﾝｶｸ);',
    '}',
    '',
  ].join('\n');
  let doc = makeDoc('prog.nihon', text);
  await ext._internals.check(doc);
  let ds = diagsOf(doc);
  const zen = ds.find((d) => d.code === 'E0002');
  assert.ok(zen, 'E0002 ausente');
  assert.deepStrictEqual(
    [zen.range.start.line, zen.range.start.character, zen.range.end.line, zen.range.end.character],
    [1, 11, 1, 13]
  );
  assert.strictEqual(zen.source, 'nihonc');

  // ---------- 2. quick fix de um diagnóstico
  const acts = provider.provideCodeActions(doc, zen.range, { diagnostics: [zen] });
  const fix = acts.find((a) => a.kind === Kind.QuickFix);
  assert.strictEqual(fix.title, 'Use the ASCII equivalents: `1;`');
  assert.strictEqual(fix.isPreferred, true);
  assert.strictEqual(apply(text, fix.edit.edits).split('\n')[1], '    束縛 x = 1;');

  // ---------- 3. "fix all": repete até não sobrar nada com correção segura
  for (let round = 1; round <= 5; round++) {
    const all = ext._internals.fixAllEdit(doc);
    if (!all) break;
    text = apply(text, all.edit.edits);
    doc = makeDoc('prog.nihon', text);
    await ext._internals.check(doc);
  }
  assert.strictEqual(
    text,
    [
      '関数 主() {',
      '    束縛 x = 1;',
      '    もし x > 0 { 表示!("a"); }',
      '    束縛 z = 3;',
      '    束縛 ハンカク = 4;',
      '    表示!("{} {}", z, ハンカク);',
      '}',
      '',
    ].join('\n')
  );
  const final = nihonc(doc.fileName);
  assert.strictEqual(final.status, 0, final.stderr);
  assert.strictEqual(final.stderr, '', 'o programa corrigido nao deveria ter avisos');
  assert.strictEqual(diagsOf(doc).length, 0);

  // ---------- 4. correção não segura aparece como quick fix, mas fora do "fix all"
  const doc2 = makeDoc('typo.nihon', '関数 主() {\n    束縛 総計 = 1;\n    表示!("{}", 総継);\n}\n');
  await ext._internals.check(doc2);
  const typo = diagsOf(doc2)[0];
  assert.strictEqual(typo.code, 'E0201');
  const typoActs = provider.provideCodeActions(doc2, typo.range, { diagnostics: [typo] });
  const similar = typoActs.find((a) => a.kind === Kind.QuickFix);
  assert.strictEqual(similar.title, 'A value with a similar name exists: `総計`');
  assert.strictEqual(similar.isPreferred, false);
  assert.strictEqual(ext._internals.fixAllEdit(doc2), null);

  // ---------- 5. erro de sintaxe com inserção (E0102 `;`) e E0106 (` 中`)
  let text3 = '関数 主() {\n    各 i 0..3 { 表示!("{}", i); }\n    束縛 q = 5\n    表示!("{}", q);\n}\n';
  let doc3 = makeDoc('syntax.nihon', text3);
  await ext._internals.check(doc3);
  for (let i = 0; i < 4; i++) {
    const all = ext._internals.fixAllEdit(doc3);
    if (!all) break;
    text3 = apply(text3, all.edit.edits);
    doc3 = makeDoc('syntax.nihon', text3);
    await ext._internals.check(doc3);
  }
  assert.ok(text3.includes('各 i 中 0..3'), text3);
  assert.ok(text3.includes('束縛 q = 5;'), text3);
  assert.strictEqual(nihonc(doc3.fileName).status, 0);

  // ---------- 6. a checagem usa --check: o nihonc novo, sem ele, deixaria um executavel no cwd
  for (const n of ['prog', 'typo', 'syntax']) {
    assert.ok(!fs.existsSync(path.join(process.cwd(), n)), `executavel ${n} foi gerado`);
  }

  fs.rmSync(dir, { recursive: true, force: true });
  console.log('e2e.js: ok');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
