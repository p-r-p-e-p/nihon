// Confere que as tabelas da extensão, a gramática e os snippets andam juntos.
// As tabelas espelham lexer/keywords.rs e a tabela TYPES do codegen_c.rs do nihonc.
// Não precisa do nihonc nem do VS Code: usa um `vscode` de mentira.
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const Module = require('module');

const root = path.join(__dirname, '..');
const kind = { append() { return kind; }, contains() { return true; }, intersects() { return true; } };
const stub = { CodeActionKind: { QuickFix: kind, SourceFixAll: kind } };
const origLoad = Module._load;
Module._load = function (req, ...a) { return req === 'vscode' ? stub : origLoad.call(this, req, ...a); };
const { tables } = require('../extension')._internals;
Module._load = origLoad;

const json = (p) => JSON.parse(fs.readFileSync(path.join(root, p), 'utf8'));

// ---- tabelas: tamanhos esperados e sem duplicados (como o teste do compilador)
assert.strictEqual(tables.KEYWORDS.length, 28);
assert.strictEqual(tables.MACROS.length, 6);
assert.strictEqual(tables.TYPES.length, 14);
assert.strictEqual(tables.METHODS.length, 2);
const seen = new Set();
for (const list of Object.values(tables)) {
  for (const [jp, ro, en] of list) {
    assert.ok(jp && ro && en, `entrada incompleta: ${jp}`);
    assert.ok(!seen.has(jp), `duplicado: ${jp}`);
    assert.ok(!seen.has(ro), `duplicado: ${ro}`);
    seen.add(jp); seen.add(ro);
  }
}

// ---- gramática: toda palavra-chave, tipo e método (jp e romaji) está num regex
const tokens = new Set();
(function walk(o) {
  if (Array.isArray(o)) return o.forEach(walk);
  if (o && typeof o === 'object') {
    for (const [k, v] of Object.entries(o)) {
      if (k === 'match' && typeof v === 'string') v.split(/[|()]/).forEach((t) => tokens.add(t));
      else walk(v);
    }
  }
})(json('syntaxes/nihon.tmLanguage.json'));
for (const list of [tables.KEYWORDS, tables.TYPES, tables.METHODS]) {
  for (const [jp, ro] of list) {
    assert.ok(tokens.has(jp), `gramática sem \`${jp}\``);
    assert.ok(tokens.has(ro), `gramática sem \`${ro}\``);
  }
}
// macros são reconhecidas pelo padrão genérico `nome!(`
const macroRule = json('syntaxes/nihon.tmLanguage.json').repository.macros.match;
assert.ok(macroRule.includes('(!)'), 'regra de macro mudou');

// regras novas para enums e `照合`
const rep = json('syntaxes/nihon.tmLanguage.json').repository;
assert.ok(rep.wildcard && rep.paths, 'regras wildcard/paths ausentes');
assert.ok(json('syntaxes/nihon.tmLanguage.json').patterns.some((p) => p.include === '#wildcard'));
assert.ok(json('syntaxes/nihon.tmLanguage.json').patterns.some((p) => p.include === '#paths'));

// ---- snippets
const snippets = json('snippets/nihon.json');
const prefixes = new Set();
for (const [name, s] of Object.entries(snippets)) {
  assert.ok(s.body && s.description, `snippet incompleto: ${name}`);
  const list = Array.isArray(s.prefix) ? s.prefix : [s.prefix];
  assert.ok(list.length > 0, name);
  for (const p of list) {
    assert.ok(!prefixes.has(p), `prefixo repetido: ${p}`);
    prefixes.add(p);
  }
}
for (const p of ['kouzoutai', 'jissou', 'hokamoshi', 'henkyaku', 'seikei', 'shutsuryoku', 'rekkyogata', 'shougou']) {
  assert.ok(prefixes.has(p), `snippet ${p} ausente`);
}

// ---- package.json aponta para arquivos que existem
const pkg = json('package.json');
for (const p of [pkg.main, pkg.contributes.grammars[0].path, pkg.contributes.snippets[0].path, pkg.contributes.languages[0].configuration]) {
  assert.ok(fs.existsSync(path.join(root, p)), `arquivo ausente: ${p}`);
}
assert.ok(/^\d+\.\d+\.\d+$/.test(pkg.version));

console.log('tables.test.js: ok');
