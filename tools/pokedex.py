#!/usr/bin/env python3
"""Genera `skins.json` para claude-pet a partir de los iconos de PokeAPI.

Toma el icono de menú de Generación VII (40×30, ya es pixel art con fondo
transparente), lo recorta, lo encaja en una rejilla de GRID×GRID, cuantiza la
paleta, busca los ojos y lo escribe en el formato de avatar de la app (filas de
caracteres + paleta + celdas de ojos), el que lee `src/skins.rs`.

Uso:
  python3 tools/pokedex.py --gen 1                       # los 151 de Kanto
  python3 tools/pokedex.py --ids 1,4,7,25 --preview out.png
  python3 tools/pokedex.py --ids 1-151 --grid 16         # rejilla chica
  claude-pet skins skins.json                            # instalarlo

Requiere Pillow (`pip install pillow` o `uv pip install pillow`). Las descargas
se guardan en tools/.cache/ para no repetirlas. Los sprites son propiedad de
Nintendo / Game Freak / Creatures: no subas el JSON ni los previews al repo.
"""
import argparse
import io
import json
import sys
import urllib.request
from pathlib import Path

from PIL import Image

ICON = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/versions/generation-vii/icons/{id}.png"
API = "https://pokeapi.co/api/v2/"
GENS = {1: (1, 151), 2: (152, 251), 3: (252, 386), 4: (387, 493), 5: (494, 649),
        6: (650, 721), 7: (722, 809), 8: (810, 905), 9: (906, 1025)}
CHARS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
CACHE = Path(__file__).resolve().parent / ".cache"


def get(url: str) -> bytes:
    key = CACHE / url.replace("https://", "").replace("/", "_")
    if key.exists():
        return key.read_bytes()
    CACHE.mkdir(exist_ok=True)
    # PokeAPI responde 403 a peticiones sin User-Agent.
    req = urllib.request.Request(url, headers={"User-Agent": "claude-pet-pokedex/0.2"})
    data = urllib.request.urlopen(req, timeout=30).read()
    key.write_bytes(data)
    return data


def spanish_name(pid: int) -> str:
    try:
        d = json.loads(get(API + f"pokemon-species/{pid}"))
        for n in d["names"]:
            if n["language"]["name"] == "es":
                return n["name"]
        return d["name"].capitalize()
    except Exception:
        return f"#{pid}"


def pixels(im: Image.Image) -> list:
    # Pillow 12 renombró getdata; las versiones viejas no tienen el nuevo.
    return list(im.get_flattened_data() if hasattr(im, "get_flattened_data") else im.getdata())


def downscale(im: Image.Image, nw: int, nh: int):
    """Promedio por bloque ponderado por alpha (evita bordes oscuros). Devuelve
    una matriz nh×nw de (r, g, b) o None si el bloque es mayormente transparente."""
    w, h = im.size
    px = im.load()
    out = []
    for ty in range(nh):
        row = []
        y0, y1 = ty * h // nh, max(ty * h // nh + 1, (ty + 1) * h // nh)
        for tx in range(nw):
            x0, x1 = tx * w // nw, max(tx * w // nw + 1, (tx + 1) * w // nw)
            r = g = b = a = 0
            n = 0
            for y in range(y0, y1):
                for x in range(x0, x1):
                    pr, pg, pb, pa = px[x, y]
                    r += pr * pa
                    g += pg * pa
                    b += pb * pa
                    a += pa
                    n += 1
            if a == 0 or a < 128 * n:
                row.append(None)
            else:
                row.append((r // a, g // a, b // a))
        out.append(row)
    return out


def quantize(cells, colors: int):
    """Reduce los colores opacos a `colors` con median cut, sin dither."""
    opaque = [(y, x) for y, row in enumerate(cells) for x, c in enumerate(row) if c]
    if not opaque:
        return cells
    strip = Image.new("RGB", (len(opaque), 1))
    strip.putdata([cells[y][x] for y, x in opaque])
    q = strip.quantize(colors=min(colors, len(opaque)), method=Image.Quantize.MEDIANCUT,
                       dither=Image.Dither.NONE).convert("RGB")
    out = [row[:] for row in cells]
    for (y, x), c in zip(opaque, pixels(q)):
        out[y][x] = tuple(c)
    return out


def pixelate(im: Image.Image, grid: int, colors: int):
    im = im.convert("RGBA")
    im = im.crop(im.getbbox())
    w, h = im.size
    s = min(1.0, grid / max(w, h))          # nunca ampliar: si cabe, va a tamaño real
    nw, nh = max(1, round(w * s)), max(1, round(h * s))
    cells = quantize(downscale(im, nw, nh), colors)
    ox, oy = (grid - nw) // 2, grid - nh    # centrado, pies en la última fila
    rows = [["."] * grid for _ in range(grid)]
    palette = {}
    by_color = {}
    for y, row in enumerate(cells):
        for x, c in enumerate(row):
            if c is None:
                continue
            if c not in by_color:
                ch = CHARS[len(by_color)]
                by_color[c] = ch
                palette[ch] = list(c)
            rows[oy + y][ox + x] = by_color[c]
    return ["".join(r) for r in rows], palette


def find_eyes(rows, palette):
    """Celdas (x, y) de los ojos, para que la app los cierre al parpadear:
    manchas de hasta 2×2 del color más oscuro, en los dos tercios de arriba
    del sprite y rodeadas de cuerpo (el contorno es grande y toca lo
    transparente, así que no cuenta). Si no salen uno o dos ojos, nada."""
    if not palette:
        return []
    dark = min(palette, key=lambda ch: 0.299 * palette[ch][0] + 0.587 * palette[ch][1] + 0.114 * palette[ch][2])
    g = len(rows)
    ys = [y for y, r in enumerate(rows) if any(ch != "." for ch in r)]
    if not ys:
        return []
    limit = ys[0] + (ys[-1] - ys[0] + 1) * 2 // 3
    around = lambda x, y: [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)]
    inside = lambda x, y: 0 <= x < g and 0 <= y < g
    seen = set()
    eyes = []
    for y in range(ys[0], limit):
        for x in range(g):
            if rows[y][x] != dark or (x, y) in seen:
                continue
            blob, stack = [], [(x, y)]
            while stack:
                cx, cy = stack.pop()
                if (cx, cy) in seen or not inside(cx, cy) or rows[cy][cx] != dark:
                    continue
                seen.add((cx, cy))
                blob.append((cx, cy))
                stack += around(cx, cy)
            xs, ys_ = [p[0] for p in blob], [p[1] for p in blob]
            if len(blob) > 4 or max(xs) - min(xs) > 1 or max(ys_) - min(ys_) > 1:
                continue
            if any(not inside(nx, ny) or rows[ny][nx] == "." for cx, cy in blob for nx, ny in around(cx, cy)):
                continue
            eyes.append(blob)
    if not 1 <= len(eyes) <= 2:
        return []
    return sorted(p for b in eyes for p in b)


def render(rows, palette, scale: int) -> Image.Image:
    grid = len(rows)
    out = Image.new("RGB", (grid * scale, grid * scale), (52, 55, 68))
    px = out.load()
    for y, row in enumerate(rows):
        for x, ch in enumerate(row):
            if ch == ".":
                continue
            col = tuple(palette[ch])
            for dy in range(scale):
                for dx in range(scale):
                    px[x * scale + dx, y * scale + dy] = col
    return out


def parse_ids(spec: str):
    ids = []
    for part in spec.split(","):
        part = part.strip()
        if "-" in part:
            a, b = part.split("-")
            ids.extend(range(int(a), int(b) + 1))
        elif part:
            ids.append(int(part))
    return ids


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--ids", help="lista o rangos: 1,4,7 o 1-151")
    ap.add_argument("--gen", type=int, action="append", help="generación completa (repetible)")
    ap.add_argument("--grid", type=int, default=24, help="celdas por lado (24 = el de la app; menos se centra)")
    ap.add_argument("--colors", type=int, default=12, help="colores máximos por avatar")
    ap.add_argument("--out", default="skins.json")
    ap.add_argument("--preview", help="PNG con todos los avatares generados")
    ap.add_argument("--no-names", action="store_true", help="no consultar nombres en PokeAPI")
    ap.add_argument("--no-eyes", action="store_true", help="no buscar ojos (no parpadean)")
    args = ap.parse_args()

    ids = parse_ids(args.ids) if args.ids else []
    for g in args.gen or []:
        a, b = GENS[g]
        ids.extend(range(a, b + 1))
    ids = sorted(set(ids))
    if not ids:
        ap.error("indica --ids o --gen")
    if not 1 <= args.grid <= 24:
        ap.error("--grid debe estar entre 1 y 24")

    skins = []
    with_eyes = 0
    for pid in ids:
        try:
            im = Image.open(io.BytesIO(get(ICON.format(id=pid))))
        except Exception as e:
            print(f"#{pid}: sin icono ({e})", file=sys.stderr)
            continue
        rows, palette = pixelate(im, args.grid, args.colors)
        name = f"#{pid}" if args.no_names else spanish_name(pid)
        skin = {"id": pid, "name": name, "rows": rows, "palette": palette}
        eyes = [] if args.no_eyes else find_eyes(rows, palette)
        if eyes:
            skin["eyes"] = [list(p) for p in eyes]
            with_eyes += 1
        skins.append(skin)
        print(f"#{pid:04d} {name}: {len(palette)} colores" + (f", ojos en {eyes}" if eyes else ""))

    Path(args.out).write_text(json.dumps({"grid": args.grid, "skins": skins}, ensure_ascii=False, indent=1) + "\n")
    print(f"{len(skins)} avatares en {args.out} ({with_eyes} con ojos)")

    if args.preview and skins:
        scale, cols, pad = 6, 10, 8
        cell = args.grid * scale + pad
        rows_n = (len(skins) + cols - 1) // cols
        sheet = Image.new("RGB", (cols * cell + pad, rows_n * cell + pad), (36, 38, 48))
        for i, s in enumerate(skins):
            sheet.paste(render(s["rows"], s["palette"], scale), (pad + (i % cols) * cell, pad + (i // cols) * cell))
        sheet.save(args.preview)
        print(f"preview en {args.preview}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
