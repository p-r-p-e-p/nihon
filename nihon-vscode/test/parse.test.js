// Testes de parse.js e fixes.js com saída real do nihonc (test/fixtures).
const assert = require('assert');
const fs = require('fs');
const path = require('path');
const { parseOutput, parseJson } = require('../parse');
const { toUtf16Column, fixTitle, pickNonOverlapping } = require('../fixes');

const fx = (n) => fs.readFileSync(path.join(__dirname, 'fixtures', n), 'utf8');

// ---- JSON
let d = parseJson(fx('a.json.txt'));
assert.deepStrictEqual(d.map((x) => x.code), ['E0002', 'E0003', 'W0001', 'W0005']);
assert.strictEqual(d[0].severity, 'error');
assert.strictEqual(d[2].severity, 'warning');
assert.deepStrictEqual(d[0].start, { line: 2, col: 12 });
assert.deepStrictEqual(d[0].end, { line: 2, col: 14 });
assert.strictEqual(d[0].fixes[0].replacement, '1;');
assert.strictEqual(d[0].fixes[0].safe, true);
assert.strictEqual(d[1].fixes[0].replacement, ' ');           // inserção: start == end
assert.deepStrictEqual(d[1].fixes[0].start, d[1].fixes[0].end);
assert.strictEqual(d[2].fixes[0].replacement, '関数');
assert.ok(d[2].extras.some((e) => e.startsWith('note: pass `--romaji`')));
assert.strictEqual(d[3].fixes[0].replacement, 'ハンカク');

d = parseJson(fx('e.json.txt'));
assert.strictEqual(d[0].code, 'E0201');
assert.strictEqual(d[0].fixes[0].safe, false);                  // "nome parecido" é só dica
assert.strictEqual(d[0].fixes[0].replacement, '総計');

d = parseJson(fx('d.json.txt'));
assert.deepStrictEqual(d.map((x) => x.code), ['E0106', 'E0102']);
assert.strictEqual(d[0].fixes[0].replacement, ' 中');

d = parseJson(fx('c.json.txt'));
assert.strictEqual(d[0].code, 'W0004');
assert.strictEqual(d[0].fixes[0].replacement, '');              // remover

d = parseJson(fx('m.json.txt'));
assert.deepStrictEqual(d.map((x) => x.code), ['W0001', 'E0613', 'W0001']);
assert.strictEqual(d[0].fixes[0].replacement, '長さ');           // método em romaji -> forma canônica
assert.strictEqual(d[0].fixes[0].safe, true);
assert.strictEqual(d[1].fixes[0].replacement, '長さ');           // método parecido: só dica
assert.strictEqual(d[1].fixes[0].safe, false);
assert.strictEqual(d[2].fixes[0].replacement, '文字列');         // tipo em romaji

// linhas que não são JSON e sem posição não quebram
d = parseJson('lixo\n{"level":"error","code":"E0204","message":"no main","spans":[],"notes":[],"helps":[]}\n{quebrado');
assert.strictEqual(d.length, 1);
assert.deepStrictEqual(d[0].start, { line: 1, col: 1 });
assert.strictEqual(d[0].end, null);

// ---- saída humana (compiladores antigos)
d = parseOutput(fx('a.human.txt'));
assert.deepStrictEqual(d.map((x) => x.code), ['E0002', 'E0003', 'W0001', 'W0005']);
assert.deepStrictEqual(d[0].start, { line: 2, col: 12 });
assert.strictEqual(d[0].end, null);
assert.deepStrictEqual(d[0].fixes, []);
assert.ok(d[2].extras.includes('note: pass `--romaji` to allow latin keywords'));
d = parseOutput(fx('d.human.txt'));
assert.deepStrictEqual(d.map((x) => x.code), ['E0106', 'E0102']);

// ---- fixes.js
assert.strictEqual(toUtf16Column('    束縛 x = １；', 12), 11);
assert.strictEqual(toUtf16Column('a𠀀b', 3), 3);                // 𠀀 (U+20000) ocupa 2 unidades UTF-16
assert.strictEqual(fixTitle('use the canonical form', '関数'), 'Use the canonical form: `関数`');
assert.strictEqual(fixTitle('add `;` here', ';'), 'Add `;` here');
assert.strictEqual(fixTitle('remove this `可変`', ''), 'Remove this `可変`');
const r = (a, b, rep) => ({ range: { start: { line: 0, ch: a }, end: { line: 0, ch: b } }, replacement: rep });
assert.deepStrictEqual(
  pickNonOverlapping([r(5, 8, 'x'), r(0, 3, 'a'), r(2, 4, 'b'), r(0, 3, 'a'), r(8, 8, '+'), r(8, 8, '+')]).map((x) => x.replacement),
  ['a', 'x', '+']
);

console.log('parse.test.js: ok');
