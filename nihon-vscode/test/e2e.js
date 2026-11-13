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
  CompletionItemKind: { Keyword: 0, Function: 1, Class: 2, Method: 3 }, ViewColumn: { Beside: 2 },
  languages: {
    createDiagnosticCollection: () => {
      const m = new Map();
      collections.set('nihon', m);
      return { set: (u, v) => m.set(u.toString(), v), get: (u) => m.get(u.toString()), delete: (u) => m.delete(u.toString()), dispose() {} };
    },
    registerCodeActionsProvider: (s, p) => { registered.codeActions = p; return noop(); },
    registerHoverProvider: noop,
    registerCompletionItemProvider: (s, p) => { registered.completion = p; return noop(); },
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

  // ---------- 7. tipo e método em romaji (`文字列`, `長さ`): fix all deixa o programa limpo
  let text4 = '関数 主() {\n    束縛 s: mojiretsu = "あい";\n    表示!("{}", s.nagasa());\n}\n';
  let doc4 = makeDoc('texto.nihon', text4);
  await ext._internals.check(doc4);
  assert.deepStrictEqual(diagsOf(doc4).map((d) => d.code).sort(), ['W0001', 'W0001']);
  text4 = apply(text4, ext._internals.fixAllEdit(doc4).edit.edits);
  assert.strictEqual(text4, '関数 主() {\n    束縛 s: 文字列 = "あい";\n    表示!("{}", s.長さ());\n}\n');
  doc4 = makeDoc('texto.nihon', text4);
  assert.strictEqual(nihonc(doc4.fileName).stderr, '');

  // método com erro de digitação: só quick fix, nunca no fix all
  const doc5 = makeDoc('typo2.nihon', '関数 主() {\n    束縛 s = "a";\n    表示!("{}", s.nagas());\n}\n');
  await ext._internals.check(doc5);
  const m5 = diagsOf(doc5).find((d) => d.code === 'E0613');
  const a5 = provider.provideCodeActions(doc5, m5.range, { diagnostics: [m5] }).find((a) => a.kind === Kind.QuickFix);
  assert.strictEqual(a5.title, 'A method with a similar name exists: `長さ`');
  assert.strictEqual(ext._internals.fixAllEdit(doc5), null);

  // ---------- 8. autocompletar: depois do ponto só métodos; fora dele, o resto
  const complete = (line, ch) => registered.completion.provideCompletionItems(
    { lineAt: () => ({ text: line }) }, new Position(0, ch));
  const afterDot = complete('    s.na', 8).map((i) => i.label);
  assert.deepStrictEqual(afterDot, ['長さ()', 'バイト数()']);
  const plainList = complete('    kan', 7).map((i) => i.label);
  assert.ok(plainList.includes('関数') && plainList.includes('文字列') && plainList.includes('表示!'));
  assert.ok(!plainList.includes('長さ()'));
  // todos os tipos do compilador aparecem no autocompletar
  for (const t of ['整8', '整16', '整32', '整64', '非負8', '非負16', '非負32', '非負64', '大きさ', '実32', '実64', '真偽', '文字', '文字列']) {
    assert.ok(plainList.includes(t), `tipo ${t} ausente do autocompletar`);
  }
  // digitar o romaji acha o tipo (filterText)
  const ookisa = complete('    ook', 7).find((i) => i.label === '大きさ');
  assert.ok(ookisa && ookisa.filterText.includes('ookisa'));
  assert.ok(complete('    0..', 7).map((i) => i.label).includes('関数')); // `..` (intervalo) não abre a lista de métodos

  // ---------- 9. enums e `照合`: variante com nome parecido (só dica) e `照合` que não cobre tudo
  const doc6 = makeDoc('enum1.nihon', '列挙型 色 { 赤, 緑 }\n関数 主() {\n    束縛 c = 色::赤;\n    照合 c {\n        色::黒 => {},\n        _ => {},\n    }\n}\n');
  await ext._internals.check(doc6);
  const v6 = diagsOf(doc6).find((d) => d.code === 'E0202');
  assert.ok(v6, 'E0202 (variante inexistente) ausente');
  const a6 = provider.provideCodeActions(doc6, v6.range, { diagnostics: [v6] }).find((a) => a.kind === Kind.QuickFix);
  assert.strictEqual(a6.title, 'A variant with a similar name exists: `赤`');
  assert.strictEqual(a6.isPreferred, false);
  assert.strictEqual(ext._internals.fixAllEdit(doc6), null);

  const doc7 = makeDoc('enum2.nihon', '列挙型 色 { 赤, 緑 }\n関数 主() {\n    束縛 c = 色::赤;\n    照合 c {\n        色::赤 => {},\n    }\n}\n');
  await ext._internals.check(doc7);
  assert.ok(diagsOf(doc7).some((d) => d.code === 'E0640'), 'E0640 (照合 nao cobre todos os casos) ausente');
  assert.strictEqual(ext._internals.fixAllEdit(doc7), null);

  fs.rmSync(dir, { recursive: true, force: true });
  console.log('e2e.js: ok');
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
