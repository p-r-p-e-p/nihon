#!/usr/bin/env python3
"""Gera src/width_table.rs com as larguras de coluna dos caracteres (0, 2).

Largura 0: marcas combinantes e caracteres de formatacao (Mn, Me, Cf).
Largura 2: East Asian Width W ou F (kanji, kana, hangul, full-width, emoji).
Todo o resto tem largura 1 (tabulacao e tratada a parte no codigo).

Uso: python3 tools/gen_width.py > src/width_table.rs
"""
import sys
import unicodedata


def width(cp):
    c = chr(cp)
    cat = unicodedata.category(c)
    if cat in ("Mn", "Me", "Cf"):
        return 0
    if cat in ("Cc", "Cs", "Co", "Cn"):
        return 1
    if unicodedata.east_asian_width(c) in ("W", "F"):
        return 2
    return 1


def ranges(target):
    out = []
    start = None
    for cp in range(0x110000):
        if 0xD800 <= cp <= 0xDFFF:
            hit = False
        else:
            hit = width(cp) == target
        if hit and start is None:
            start = cp
        elif not hit and start is not None:
            out.append((start, cp - 1))
            start = None
    if start is not None:
        out.append((start, 0x10FFFF))
    return out


def emit(name, doc, rs):
    print(f"/// {doc}")
    print(f"pub const {name}: &[(u32, u32)] = &[")
    line = "   "
    for a, b in rs:
        item = f" (0x{a:X}, 0x{b:X}),"
        if len(line) + len(item) > 96:
            print(line)
            line = "   "
        line += item
    if line.strip():
        print(line)
    print("];")
    print()


print("// ARQUIVO GERADO por tools/gen_width.py (Unicode %s). Nao edite a mao." % unicodedata.unidata_version)
print()
emit("ZERO", "Intervalos de largura 0 (marcas combinantes e formatacao).", ranges(0))
emit("WIDE", "Intervalos de largura 2 (East Asian Wide e Fullwidth).", ranges(2))
