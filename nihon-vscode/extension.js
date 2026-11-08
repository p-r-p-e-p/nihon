const vscode = require('vscode');
const cp = require('child_process');
const fs = require('fs');
const os = require('os');
const path = require('path');
const { parseOutput, parseJson } = require('./parse');
const { toUtf16Column, fixTitle, pickNonOverlapping } = require('./fixes');

// [japonês, romaji, significado] — espelha lexer/keywords.rs e a tabela TYPES do codegen_c.rs
const KEYWORDS = [
  ['関数', 'kansuu', 'function'], ['束縛', 'sokubaku', 'let'], ['可変', 'kahen', 'mutable'],
  ['定数', 'teisuu', 'constant'], ['もし', 'moshi', 'if'], ['他', 'hoka', 'else'],
  ['他もし', 'hokamoshi', 'else if'], ['間', 'aida', 'while'], ['無限', 'mugen', 'loop'],
  ['各', 'kaku', 'for each'], ['中', 'naka', 'inside'], ['中断', 'chuudan', 'break'],
  ['続行', 'zokkou', 'continue'], ['返却', 'henkyaku', 'return'], ['照合', 'shougou', 'match'],
  ['真', 'shin', 'true'], ['偽', 'gi', 'false'], ['構造体', 'kouzoutai', 'struct'],
  ['列挙型', 'rekkyogata', 'enum'], ['実装', 'jissou', 'implementation'], ['特性', 'tokusei', 'trait'],
  ['公開', 'koukai', 'public'], ['使用', 'shiyou', 'use'], ['自分', 'jibun', 'self'],
  ['自身', 'jishin', 'Self'], ['危険', 'kiken', 'unsafe'], ['外部', 'gaibu', 'external'],
  ['として', 'toshite', 'as'],
];
const MACROS = [
  ['表示', 'hyouji', 'println'], ['出力', 'shutsuryoku', 'print'], ['異常終了', 'ijoushuuryou', 'panic'],
  ['表明', 'hyoumei', 'assert'], ['整形', 'seikei', 'format'], ['未実装', 'mijissou', 'todo'],
];
const TYPES = [
  ['整32', 'sei32', '32-bit signed integer'], ['非負32', 'hifu32', '32-bit unsigned integer'],
  ['実64', 'jitsu64', '64-bit floating point'], ['真偽', 'shingi', 'boolean'],
  ['文字', 'moji', 'character'], ['文字列', 'mojiretsu', 'text (UTF-8, immutable)'],
];
// métodos: tabela METHODS de gen_method (codegen_c.rs); só `文字列` tem métodos por enquanto
const METHODS = [
  ['長さ', 'nagasa', 'length in characters'], ['バイト数', 'baitosuu', 'length in bytes'],
];

const LOOKUP = new Map();
for (const [list, kind] of [[KEYWORDS, 'keyword'], [MACROS, 'macro'], [TYPES, 'type'], [METHODS, 'method']]) {
  for (const [jp, ro, en] of list) {
    LOOKUP.set(jp, { jp, ro, en, kind });
    LOOKUP.set(ro, { jp, ro, en, kind });
  }
}

// mesma regra de identificador do lexer (chars.rs): kana, han, latim, `_`, meia largura
const WORD_CLASS = '_A-Za-z0-9\\u00C0-\\u024F\\u3005-\\u3007\\u3040-\\u30FF\\u3400-\\u4DBF\\u4E00-\\u9FFF\\uFF66-\\uFF9F';
const WORD = new RegExp(`[${WORD_CLASS}]+`);
// o cursor está logo depois de `algo.` (e não de `..`)?
const AFTER_DOT = new RegExp(`(^|[^.])\\.[${WORD_CLASS}]*$`);

const FIXALL_KIND = vscode.CodeActionKind.SourceFixAll.append('nihon');

let diagnostics;
let warnedMissing = false;
let tmpDir = null;
const tmpNames = new Map();   // uri -> nome do arquivo temporário
const seq = new Map();        // uri -> número da última checagem (descarta respostas velhas)
const timers = new Map();     // uri -> debounce
const fixStore = new Map();   // uri -> Map(chave do diagnóstico -> correções)
const capCache = new Map();   // caminho do compilador -> Promise<capacidades>

function cfg() {
  return vscode.workspace.getConfiguration('nihon');
}

function exec(file, args, cwd) {
  return new Promise((resolve) => {
    cp.execFile(
      file,
      args,
      { timeout: 15000, maxBuffer: 16 * 1024 * 1024, cwd, env: { ...process.env, NO_COLOR: '1' } },
      (err, stdout, stderr) => resolve({ err, stdout, stderr })
    );
  });
}

// Descobre o que o nihonc instalado sabe fazer (--check, --message-format) pelo --help,
// assim a extensão continua funcionando com versões antigas do compilador.
function getCaps(compiler) {
  if (!capCache.has(compiler)) {
    capCache.set(
      compiler,
      new Promise((resolve) => {
        cp.execFile(compiler, ['--help'], { timeout: 10000 }, (err, stdout, stderr) => {
          if (err && err.code === 'ENOENT') {
            capCache.delete(compiler); // o usuário pode instalar/corrigir o caminho depois
            return resolve({ missing: true });
          }
          const t = `${stdout}\n${stderr}`;
          resolve({ missing: false, check: t.includes('--check'), json: t.includes('--message-format') });
        });
      })
    );
  }
  return capCache.get(compiler);
}

function cwdFor(doc) {
  const f = vscode.workspace.getWorkspaceFolder(doc.uri);
  return f ? f.uri.fsPath : undefined;
}

// Texto ainda não salvo vai para um arquivo temporário, para o erro aparecer enquanto digita.
function bufferPath(doc) {
  if (!tmpDir) tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nihon-vscode-'));
  const key = doc.uri.toString();
  let name = tmpNames.get(key);
  if (!name) {
    name = `buffer${tmpNames.size}.nihon`;
    tmpNames.set(key, name);
  }
  const p = path.join(tmpDir, name);
  fs.writeFileSync(p, doc.getText());
  return p;
}

function toPos(doc, p) {
  const line = Math.min(Math.max(p.line - 1, 0), doc.lineCount - 1);
  return new vscode.Position(line, toUtf16Column(doc.lineAt(line).text, p.col));
}

function widen(doc, r) {
  const text = doc.lineAt(r.start.line).text;
  if (r.start.character < text.length) return new vscode.Range(r.start, new vscode.Position(r.start.line, r.start.character + 1));
  if (r.start.character > 0) return new vscode.Range(new vscode.Position(r.start.line, r.start.character - 1), r.start);
  return r;
}

function diagRange(doc, d) {
  const start = toPos(doc, d.start);
  if (!d.end) {
    // saída humana: só temos o início, então sublinha a palavra naquele ponto
    return doc.getWordRangeAtPosition(start, WORD) || widen(doc, new vscode.Range(start, start));
  }
  const r = new vscode.Range(start, toPos(doc, d.end));
  return r.isEmpty ? widen(doc, r) : r;
}

function dkey(d) {
  const r = d.range;
  return [d.code, d.message, r.start.line, r.start.character, r.end.line, r.end.character].join('|');
}

async function check(doc) {
  if (doc.languageId !== 'nihon' || doc.uri.scheme !== 'file') return;
  const key = doc.uri.toString();
  if (!cfg().get('diagnostics.enabled')) {
    diagnostics.delete(doc.uri);
    fixStore.delete(key);
    return;
  }
  const mine = (seq.get(key) || 0) + 1;
  seq.set(key, mine);

  const compiler = cfg().get('compilerPath') || 'nihonc';
  const caps = await getCaps(compiler);
  if (caps.missing) {
    if (!warnedMissing) {
      warnedMissing = true;
      vscode.window.showWarningMessage(
        'Could not find the `nihonc` compiler. Set "nihon.compilerPath" in settings (e.g. /path/to/target/release/nihonc).'
      );
    }
    return;
  }

  const args = [];
  if (caps.check) args.push('--check'); // sem isso o nihonc novo gera um executável
  if (caps.json) args.push('--message-format=json');
  if (cfg().get('strict')) args.push('--strict');
  if (cfg().get('romaji')) args.push('--romaji');
  args.push(doc.isDirty ? bufferPath(doc) : doc.fileName);

  const { stderr } = await exec(compiler, args, cwdFor(doc));
  if (seq.get(key) !== mine) return; // saiu uma checagem mais nova enquanto esta rodava

  const found = caps.json ? parseJson(stderr || '') : parseOutput(stderr || '');
  const items = [];
  const store = new Map();
  for (const d of found) {
    const diag = new vscode.Diagnostic(
      diagRange(doc, d),
      [d.message, ...d.extras].join('\n'),
      d.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
    );
    diag.code = d.code;
    diag.source = 'nihonc';
    items.push(diag);
    const fixes = d.fixes.map((f) => ({
      title: fixTitle(f.message, f.replacement),
      range: new vscode.Range(toPos(doc, f.start), toPos(doc, f.end)),
      replacement: f.replacement,
      safe: f.safe,
    }));
    if (fixes.length) store.set(dkey(diag), fixes);
  }
  diagnostics.set(doc.uri, items);
  fixStore.set(key, store);
}

function schedule(doc) {
  if (doc.languageId !== 'nihon') return;
  if (cfg().get('diagnostics.run') !== 'onType') return;
  const key = doc.uri.toString();
  clearTimeout(timers.get(key));
  timers.set(key, setTimeout(() => check(doc), 400));
}

// ---------------------------------------------------------------- quick fixes

function plain(r) {
  return { start: { line: r.start.line, ch: r.start.character }, end: { line: r.end.line, ch: r.end.character } };
}

function fixAllEdit(doc) {
  const store = fixStore.get(doc.uri.toString());
  if (!store) return null;
  const safe = [];
  for (const fixes of store.values()) {
    for (const f of fixes) if (f.safe) safe.push({ range: f.range, replacement: f.replacement });
  }
  const picked = pickNonOverlapping(
    safe.map((s) => ({ range: plain(s.range), replacement: s.replacement, orig: s }))
  ).map((p) => p.orig);
  if (!picked.length) return null;
  const edit = new vscode.WorkspaceEdit();
  for (const p of picked) edit.replace(doc.uri, p.range, p.replacement);
  return { edit, count: picked.length };
}

const codeActions = {
  providedCodeActionKinds: [vscode.CodeActionKind.QuickFix, FIXALL_KIND],

  provideCodeActions(doc, range, context) {
    const store = fixStore.get(doc.uri.toString());
    if (!store) return [];
    const out = [];
    const wantsQuick = !context.only || context.only.contains(vscode.CodeActionKind.QuickFix) ||
      vscode.CodeActionKind.QuickFix.contains(context.only);
    if (wantsQuick) {
      for (const d of context.diagnostics) {
        if (d.source !== 'nihonc') continue;
        const fixes = store.get(dkey(d));
        if (!fixes) continue;
        for (const f of fixes) {
          const a = new vscode.CodeAction(f.title, vscode.CodeActionKind.QuickFix);
          a.diagnostics = [d];
          a.edit = new vscode.WorkspaceEdit();
          a.edit.replace(doc.uri, f.range, f.replacement);
          a.isPreferred = f.safe && fixes.length === 1;
          out.push(a);
        }
      }
    }
    const all = fixAllEdit(doc);
    const wantsAll = context.only ? FIXALL_KIND.intersects(context.only) : all && all.count > 1;
    if (all && wantsAll) {
      const a = new vscode.CodeAction(`Fix all safe issues (${all.count})`, FIXALL_KIND);
      a.edit = all.edit;
      out.push(a);
    }
    return out;
  },
};

// ------------------------------------------------------------------ ativação

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection('nihon');
  context.subscriptions.push(diagnostics);

  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(check),
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidChangeTextDocument((e) => schedule(e.document)),
    vscode.workspace.onDidCloseTextDocument((d) => {
      diagnostics.delete(d.uri);
      fixStore.delete(d.uri.toString());
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (!e.affectsConfiguration('nihon')) return;
      capCache.clear();
      vscode.workspace.textDocuments.forEach(check);
    }),

    vscode.commands.registerCommand('nihon.check', () => {
      const ed = vscode.window.activeTextEditor;
      if (ed) check(ed.document);
    }),
    vscode.commands.registerCommand('nihon.fixAll', async () => {
      const ed = vscode.window.activeTextEditor;
      if (!ed || ed.document.languageId !== 'nihon') return;
      const all = fixAllEdit(ed.document);
      if (!all) {
        vscode.window.showInformationMessage('Nihon: no safe fixes to apply.');
        return;
      }
      await vscode.workspace.applyEdit(all.edit);
    }),
    vscode.commands.registerCommand('nihon.showTokens', async () => {
      const ed = vscode.window.activeTextEditor;
      if (!ed || ed.document.languageId !== 'nihon') return;
      await ed.document.save();
      const { stdout, stderr } = await exec(cfg().get('compilerPath') || 'nihonc', ['--tokens', ed.document.fileName], cwdFor(ed.document));
      const content = stdout && stdout.trim() ? stdout : stderr;
      const doc = await vscode.workspace.openTextDocument({ content, language: 'plaintext' });
      vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    }),

    vscode.languages.registerCodeActionsProvider('nihon', codeActions, {
      providedCodeActionKinds: codeActions.providedCodeActionKinds,
    }),

    vscode.languages.registerHoverProvider('nihon', {
      provideHover(doc, pos) {
        const range = doc.getWordRangeAtPosition(pos, WORD);
        if (!range) return;
        const info = LOOKUP.get(doc.getText(range));
        if (!info) return;
        const md = new vscode.MarkdownString();
        md.appendMarkdown(`**${info.jp}${info.kind === 'macro' ? '!' : ''}** · \`${info.ro}\` — ${info.en}`);
        if (info.kind !== 'keyword') md.appendMarkdown(`\n\n${info.kind}`);
        return new vscode.Hover(md, range);
      },
    }),

    // digitar "kansuu" no teclado latino e escolher 関数 na lista
    vscode.languages.registerCompletionItemProvider('nihon', {
      provideCompletionItems(doc, pos) {
        const mk = ([jp, ro, en], kind) => {
          const text = kind === 'macro' ? `${jp}!` : kind === 'method' ? `${jp}()` : jp;
          const k = kind === 'macro' ? vscode.CompletionItemKind.Function
            : kind === 'method' ? vscode.CompletionItemKind.Method
            : kind === 'type' ? vscode.CompletionItemKind.Class : vscode.CompletionItemKind.Keyword;
          const item = new vscode.CompletionItem(text, k);
          item.insertText = text;
          item.filterText = `${ro} ${jp}`;
          item.sortText = ro;
          item.detail = `${ro} — ${en}`;
          return item;
        };
        // depois de um ponto só fazem sentido os métodos
        if (AFTER_DOT.test(doc.lineAt(pos.line).text.slice(0, pos.character))) {
          return METHODS.map((x) => mk(x, 'method'));
        }
        return [
          ...KEYWORDS.map((x) => mk(x, 'keyword')),
          ...MACROS.map((x) => mk(x, 'macro')),
          ...TYPES.map((x) => mk(x, 'type')),
        ];
      },
    }, '.')
  );

  vscode.workspace.textDocuments.forEach(check);
}

function deactivate() {
  if (tmpDir) {
    try {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    } catch {
      /* ignora */
    }
  }
}

module.exports = { activate, deactivate, _internals: { check, codeActions, fixAllEdit, fixStore } };
