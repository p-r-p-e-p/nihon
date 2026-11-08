import json
B = r"(?<![\p{L}\p{N}_])"
E = r"(?![\p{L}\p{N}_])"
ID = r"[\p{L}_][\p{L}\p{N}_]*"

def alt(words):
    return "|".join(sorted(words, key=lambda w: -len(w)))

def kw(words, name):
    return {"name": name, "match": f"{B}({alt(words)}){E}"}

# (jp, romaji)
control = [("もし","moshi"),("他もし","hokamoshi"),("他","hoka"),("間","aida"),("無限","mugen"),
           ("各","kaku"),("中","naka"),("中断","chuudan"),("続行","zokkou"),("返却","henkyaku"),("照合","shougou")]
storage = [("関数","kansuu"),("束縛","sokubaku"),("定数","teisuu"),("構造体","kouzoutai"),
           ("列挙型","rekkyogata"),("実装","jissou"),("特性","tokusei")]
modifier = [("可変","kahen"),("公開","koukai"),("危険","kiken"),("外部","gaibu")]
other = [("使用","shiyou"),("として","toshite")]
consts = [("真","shin"),("偽","gi")]
selfs = [("自分","jibun"),("自身","jishin")]
macros = [("表示","hyouji"),("出力","shutsuryoku"),("異常終了","ijoushuuryou"),
          ("表明","hyoumei"),("整形","seikei"),("未実装","mijissou")]
# (jp, romaji) — tabela TYPES do codegen_c.rs
types = [("整32","sei32"),("非負32","hifu32"),("実64","jitsu64"),("真偽","shingi"),("文字","moji")]

flat = lambda pairs: [w for p in pairs for w in p]

grammar = {
 "$schema": "https://raw.githubusercontent.com/martinring/tmlanguage/master/tmlanguage.json",
 "name": "Nihon",
 "scopeName": "source.nihon",
 "patterns": [{"include": "#comments"},{"include": "#strings"},{"include": "#chars"},
              {"include": "#zenkaku"},{"include": "#halfkana"},{"include": "#numbers"},
              {"include": "#macros"},{"include": "#definitions"},{"include": "#keywords"},
              {"include": "#types"},{"include": "#calls"},{"include": "#operators"}],
 "repository": {
  "comments": {"patterns": [
    {"name": "comment.line.double-slash.nihon", "match": "//.*$"},
    {"include": "#block_comment"}]},
  "block_comment": {"name": "comment.block.nihon", "begin": "/\\*", "end": "\\*/",
                    "patterns": [{"include": "#block_comment"}]},
  "escapes": {"patterns": [
    {"name": "constant.character.escape.nihon", "match": "\\\\(?:[nrt0\\\\\"']|u\\{[0-9A-Fa-f]{1,6}\\})"},
    {"name": "invalid.illegal.escape.nihon", "match": "\\\\."}]},
  "strings": {"name": "string.quoted.double.nihon", "begin": "\"", "end": "\"",
              "patterns": [{"include": "#escapes"}]},
  "chars": {"name": "string.quoted.single.nihon", "begin": "'", "end": "'|$",
            "patterns": [{"include": "#escapes"}]},
  "zenkaku": {"name": "invalid.illegal.zenkaku.nihon",
              "match": "[\\x{3000}\\x{3001}\\x{3002}\\x{3008}-\\x{3011}\\x{3014}-\\x{301B}\\x{30FB}\\x{FF01}-\\x{FF5E}]"},
  "halfkana": {"name": "invalid.deprecated.halfkana.nihon", "match": "[\\x{FF66}-\\x{FF9F}]+"},
  "numbers": {"patterns": [
    {"name": "constant.numeric.hex.nihon", "match": "(?<![\\p{L}_])0[xX][0-9A-Fa-f_]+"},
    {"name": "constant.numeric.binary.nihon", "match": "(?<![\\p{L}_])0[bB][01_]+"},
    {"name": "constant.numeric.octal.nihon", "match": "(?<![\\p{L}_])0[oO][0-7_]+"},
    {"name": "constant.numeric.float.nihon", "match": "(?<![\\p{L}_])[0-9][0-9_]*(?:\\.[0-9][0-9_]*(?:[eE][+-]?[0-9_]+)?|[eE][+-]?[0-9_]+)"},
    {"name": "constant.numeric.integer.nihon", "match": "(?<![\\p{L}_])[0-9][0-9_]*"}]},
  "macros": {"match": f"{B}({ID})(!)(?=[(\\[{{])",
             "captures": {"1": {"name": "entity.name.function.macro.nihon"},
                          "2": {"name": "punctuation.definition.macro.nihon"}}},
  "definitions": {"patterns": [
    {"match": f"{B}({alt(['関数','kansuu'])}){E}\\s+({ID})",
     "captures": {"1": {"name": "storage.type.function.nihon"}, "2": {"name": "entity.name.function.nihon"}}},
    {"match": f"{B}({alt(['構造体','kouzoutai','列挙型','rekkyogata','特性','tokusei','実装','jissou'])}){E}\\s+({ID})",
     "captures": {"1": {"name": "storage.type.nihon"}, "2": {"name": "entity.name.type.nihon"}}}]},
  "keywords": {"patterns": [
    kw(flat(control), "keyword.control.nihon"),
    kw(flat(storage), "storage.type.nihon"),
    kw(flat(modifier), "storage.modifier.nihon"),
    kw(flat(other), "keyword.other.nihon"),
    kw(flat(consts), "constant.language.boolean.nihon"),
    kw(flat(selfs), "variable.language.self.nihon")]},
  "types": {"patterns": [
    kw(flat(types), "support.type.primitive.nihon")]},
  "calls": {"match": f"{B}({ID})(?=\\s*\\()", "captures": {"1": {"name": "entity.name.function.call.nihon"}}},
  "operators": {"name": "keyword.operator.nihon",
                "match": "->|=>|==|!=|<=|>=|&&|\\|\\||::|\\.\\.=?|[+\\-*/%]=|[+\\-*/%=<>!&|]"}
 }
}
json.dump(grammar, open("syntaxes/nihon.tmLanguage.json","w"), ensure_ascii=False, indent=2)
