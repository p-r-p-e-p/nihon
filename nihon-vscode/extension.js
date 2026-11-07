const vscode = require('vscode');
const cp = require('child_process');
const { parseOutput } = require('./parse');

// [japonês, romaji, significado] — espelha src/lexer/keywords.rs
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

const LOOKUP = new Map();
for (const [jp, ro, en] of KEYWORDS) {
  LOOKUP.set(jp, { jp, ro, en, macro: false });
  LOOKUP.set(ro, { jp, ro, en, macro: false });
}
for (const [jp, ro, en] of MACROS) {
  LOOKUP.set(jp, { jp, ro, en, macro: true });
  LOOKUP.set(ro, { jp, ro, en, macro: true });
}

// mesma regra de identificador do lexer (chars.rs): kana, han, latim, `_`, meia largura
const WORD = /[_A-Za-z0-9\u00C0-\u024F\u3005-\u3007\u3040-\u30FF\u3400-\u4DBF\u4E00-\u9FFF\uFF66-\uFF9F]+/;

let diagnostics;
let warnedMissing = false;

function cfg() {
  return vscode.workspace.getConfiguration('nihon');
}

function runCompiler(doc, extraArgs) {
  const c = cfg();
  const args = [...extraArgs];
  if (c.get('strict')) args.push('--strict');
  if (c.get('romaji')) args.push('--romaji');
  args.push(doc.fileName);
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri);
  return new Promise((resolve) => {
    cp.execFile(
      c.get('compilerPath') || 'nihonc',
      args,
      { timeout: 15000, maxBuffer: 16 * 1024 * 1024, cwd: folder ? folder.uri.fsPath : undefined, env: { ...process.env, NO_COLOR: '1' } },
      (err, stdout, stderr) => resolve({ err, stdout, stderr })
    );
  });
}

// col do nihonc conta caracteres (code points); o VS Code conta UTF-16.
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

async function check(doc) {
  if (doc.languageId !== 'nihon' || doc.uri.scheme !== 'file') return;
  if (!cfg().get('diagnostics.enabled')) {
    diagnostics.delete(doc.uri);
    return;
  }
  const { err, stderr } = await runCompiler(doc, []);
  if (err && err.code === 'ENOENT') {
    if (!warnedMissing) {
      warnedMissing = true;
      vscode.window.showWarningMessage(
        'Could not find the `nihonc` compiler. Set "nihon.compilerPath" in settings (e.g. /path/to/target/release/nihonc).'
      );
    }
    return;
  }
  const items = parseOutput(stderr || '').map((d) => {
    const line = Math.min(Math.max(d.line - 1, 0), doc.lineCount - 1);
    const startCh = toUtf16Column(doc.lineAt(line).text, d.col);
    const pos = new vscode.Position(line, startCh);
    const range = doc.getWordRangeAtPosition(pos, WORD) || new vscode.Range(pos, pos.translate(0, 1));
    const msg = [d.message, ...d.extras].join('\n');
    const diag = new vscode.Diagnostic(
      range,
      msg,
      d.severity === 'error' ? vscode.DiagnosticSeverity.Error : vscode.DiagnosticSeverity.Warning
    );
    diag.code = d.code;
    diag.source = 'nihonc';
    return diag;
  });
  diagnostics.set(doc.uri, items);
}

function activate(context) {
  diagnostics = vscode.languages.createDiagnosticCollection('nihon');
  context.subscriptions.push(diagnostics);

  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(check),
    vscode.workspace.onDidOpenTextDocument(check),
    vscode.workspace.onDidCloseTextDocument((d) => diagnostics.delete(d.uri)),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('nihon')) vscode.workspace.textDocuments.forEach(check);
    }),
    vscode.commands.registerCommand('nihon.check', () => {
      const ed = vscode.window.activeTextEditor;
      if (ed) check(ed.document);
    }),
    vscode.commands.registerCommand('nihon.showTokens', async () => {
      const ed = vscode.window.activeTextEditor;
      if (!ed || ed.document.languageId !== 'nihon') return;
      await ed.document.save();
      const { stdout, stderr } = await runCompiler(ed.document, ['--tokens']);
      const content = stdout && stdout.trim() ? stdout : stderr;
      const doc = await vscode.workspace.openTextDocument({ content, language: 'plaintext' });
      vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    }),

    vscode.languages.registerHoverProvider('nihon', {
      provideHover(doc, pos) {
        const range = doc.getWordRangeAtPosition(pos, WORD);
        if (!range) return;
        const info = LOOKUP.get(doc.getText(range));
        if (!info) return;
        const md = new vscode.MarkdownString();
        md.appendMarkdown(`**${info.jp}${info.macro ? '!' : ''}** · \`${info.ro}\` — ${info.en}`);
        if (info.macro) md.appendMarkdown('\n\nmacro');
        return new vscode.Hover(md, range);
      },
    }),

    // digitar "kansuu" no teclado latino e escolher 関数 na lista
    vscode.languages.registerCompletionItemProvider('nihon', {
      provideCompletionItems() {
        const mk = ([jp, ro, en], macro) => {
          const text = macro ? `${jp}!` : jp;
          const item = new vscode.CompletionItem(text, macro ? vscode.CompletionItemKind.Function : vscode.CompletionItemKind.Keyword);
          item.insertText = text;
          item.filterText = `${ro} ${jp}`;
          item.sortText = ro;
          item.detail = `${ro} — ${en}`;
          return item;
        };
        return [...KEYWORDS.map((k) => mk(k, false)), ...MACROS.map((m) => mk(m, true))];
      },
    })
  );

  vscode.workspace.textDocuments.forEach(check);
}

function deactivate() {}

module.exports = { activate, deactivate };
