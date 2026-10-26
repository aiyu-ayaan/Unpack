#!/usr/bin/env python3
"""Generate the Unpack brand SVGs and the wordmark path module used by the app.

Run from anywhere:  python3 assets/brand/build.py
Writes the SVGs next to this file and app/src/components/logoPaths.ts.
The wordmark is constructed (no fonts): stroked rectilinear letters, outlined here
so every file is plain filled paths. All numbers are in the 256-unit symbol grid.
"""
from pathlib import Path

HERE = Path(__file__).resolve().parent
TS_OUT = HERE.parents[1] / "app/src/components/logoPaths.ts"

INK, GREEN = "#1f2328", "#238636"
INK_DK, GREEN_DK = "#e6edf3", "#3fb950"

# ---- symbol: tray + square (baseline y=208, x-height top y=96) -------------------------------
TRAY = "M40 96H68V172A8 8 0 0 0 76 180H180A8 8 0 0 0 188 172V96H216V180A28 28 0 0 1 188 208H68A28 28 0 0 1 40 180Z"
SQUARE = dict(x=96, y=48, width=64, height=64, rx=10)
# small-size cut: heavier walls, bigger square, wider gaps (favicon, titlebar)
TRAY_S = "M40 100H84V172H172V100H216V196A28 28 0 0 1 188 224H68A28 28 0 0 1 40 196Z"
SQUARE_S = dict(x=100, y=32, width=56, height=56, rx=8)

# ---- wordmark: u n p a c k --------------------------------------------------------------------
W_STROKE, RC = 24, 16          # stroke weight; centre-line corner radius (outer 28, inner 4)
XH, ASC, DESC = 112, 160, 56   # x-height, ascender, descender (tray body = x-height, square top = ascender)
BASE = 208


def fmt(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def stroke_outline(pts, rc=RC, w=W_STROKE):
    """Outline of an open rectilinear polyline (butt caps) with rounded corners, as an SVG path."""
    def unit(a, b):
        dx, dy = b[0] - a[0], b[1] - a[1]
        n = abs(dx) + abs(dy)
        return (dx / n, dy / n)

    dirs = [unit(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]

    def side(sign):
        nrm = lambda d: (d[1] * sign, -d[0] * sign)
        off = w / 2
        p0 = (pts[0][0] + nrm(dirs[0])[0] * off, pts[0][1] + nrm(dirs[0])[1] * off)
        ops, seq = [], [p0]
        for i in range(1, len(pts) - 1):
            di, do = dirs[i - 1], dirs[i]
            cross = di[0] * do[1] - di[1] * do[0]
            outer = (cross > 0) == (sign > 0)
            r = rc + off if outer else rc - off
            a = (pts[i][0] - di[0] * rc + nrm(di)[0] * off, pts[i][1] - di[1] * rc + nrm(di)[1] * off)
            b = (pts[i][0] + do[0] * rc + nrm(do)[0] * off, pts[i][1] + do[1] * rc + nrm(do)[1] * off)
            ops += [("L", a), ("A", r, 1 if cross > 0 else 0, b)]
            seq += [a, b]
        last = dirs[-1]
        e = (pts[-1][0] + nrm(last)[0] * off, pts[-1][1] + nrm(last)[1] * off)
        ops.append(("L", e))
        seq.append(e)
        return ops, seq

    lops, lseq = side(1)
    rops, rseq = side(-1)
    d = [f"M{fmt(lseq[0][0])} {fmt(lseq[0][1])}"]
    for op in lops:
        d.append(seg(op))
    # walk the right side backwards: each op flips direction (arcs flip sweep)
    d.append(f"L{fmt(rseq[-1][0])} {fmt(rseq[-1][1])}")
    # rebuild reversed ops from the sequence of end points
    starts = [rseq[0]] + [op[-1] for op in rops[:-1]]
    for op, start in zip(reversed(rops), reversed(starts)):
        if op[0] == "L":
            d.append(f"L{fmt(start[0])} {fmt(start[1])}")
        else:
            d.append(f"A{fmt(op[1])} {fmt(op[1])} 0 0 {1 - op[2]} {fmt(start[0])} {fmt(start[1])}")
    d.append("Z")
    return "".join(d)


def seg(op):
    if op[0] == "L":
        return f"L{fmt(op[1][0])} {fmt(op[1][1])}"
    return f"A{fmt(op[1])} {fmt(op[1])} 0 0 {op[2]} {fmt(op[3][0])} {fmt(op[3][1])}"


def rect(x0, y0, x1, y1):
    return f"M{fmt(x0)} {fmt(y0)}H{fmt(x1)}V{fmt(y1)}H{fmt(x0)}Z"


def letters(x):
    """Return (list of (name, path d), advance) laid out from outer-left x, on BASE."""
    w, hw = W_STROKE, W_STROKE / 2
    top, bot = BASE - XH, BASE
    gap = 26
    out = []

    def add(name, parts, width):
        nonlocal x
        out.append((name, "".join(parts)))
        x += width + gap

    # u: the tray itself, in letter size
    W = 96
    add("u", [stroke_outline([(x + hw, top), (x + hw, bot - hw), (x + W - hw, bot - hw), (x + W - hw, top)])], W)
    # n: u upside down
    add("n", [stroke_outline([(x + hw, bot), (x + hw, top + hw), (x + W - hw, top + hw), (x + W - hw, bot)])], W)
    # p: bowl + stem down to the descender
    add("p", [rect(x, top, x + w, bot + DESC),
              stroke_outline([(x + hw, top + hw), (x + W - hw, top + hw), (x + W - hw, bot - hw), (x + hw, bot - hw)])], W)
    # a: single-storey, bowl + right stem
    add("a", [rect(x + W - w, top, x + W, bot),
              stroke_outline([(x + W - hw, top + hw), (x + hw, top + hw), (x + hw, bot - hw), (x + W - hw, bot - hw)])], W)
    # c: open bowl, terminals flush with the outer edge
    Wc = 92
    add("c", [stroke_outline([(x + Wc, top + hw), (x + hw, top + hw), (x + hw, bot - hw), (x + Wc, bot - hw)])], Wc)
    # k: tall stem, two 45-degree arms meeting on it
    Wk, tw = 68, W_STROKE * 2 ** 0.5
    ym, xs = (top + bot) / 2, x + hw
    xr = x + Wk
    arm_up = [(xr - tw, top), (xr, top), (xs, top + (xr - xs)), (xs, top + (xr - tw - xs))]
    arm_dn = [(xr - tw, bot), (xr, bot), (xs, bot - (xr - xs)), (xs, bot - (xr - tw - xs))]
    poly = lambda pts: "M" + "L".join(f"{fmt(a)} {fmt(b)}" for a, b in pts) + "Z"
    add("k", [rect(x, BASE - ASC, x + w, bot), poly(arm_up), poly(arm_dn[::-1])], Wk)  # same winding, or the overlap cancels
    return out, x - gap


def svg(vb, body, title, extra=""):
    x, y, w, h = vb
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{x} {y} {w} {h}" width="{w}" height="{h}"{extra}>\n'
            f"<title>{title}</title>\n{body}\n</svg>\n")


def sym(ink, green, small=False):
    t, s = (TRAY_S, SQUARE_S) if small else (TRAY, SQUARE)
    sq = " ".join(f'{k}="{v}"' for k, v in s.items())
    return f'<path fill="{ink}" d="{t}"/>\n<rect fill="{green}" {sq}/>'


def animated(ls, vb):
    """Looping lockup for the README: CSS keyframes inside the SVG, theme-aware, 6 s cycle."""
    T = 6.0
    p = lambda sec: f"{sec / T * 100:.2f}%"
    css = [
        ".t,.l{fill:%s}.s{fill:%s}" % (INK, GREEN),
        "@media (prefers-color-scheme:dark){.t,.l{fill:%s}.s{fill:%s}}" % (INK_DK, GREEN_DK),
        ".t,.s,.l{transform-box:fill-box;transform-origin:50%% 100%%;animation-duration:%ss;animation-iteration-count:infinite;animation-fill-mode:both}" % T,
        ".all{animation:fade %ss infinite}" % T,
        "@keyframes fade{0%%,%s{opacity:1}100%%{opacity:0}}" % p(5.3),
        ".t{animation-name:t;animation-timing-function:ease-out}",
        "@keyframes t{0%%,%s{opacity:0;transform:translateY(-36px) scale(.86)}%s{opacity:1;transform:none}%s{transform:scale(1.03,.95)}%s{transform:scale(.99,1.01)}%s,100%%{transform:none}}"
        % (p(.1), p(.56), p(.72), p(.86), p(1.3)),
        ".s{animation-name:s;animation-timing-function:cubic-bezier(.3,1.4,.5,1)}",
        "@keyframes s{0%%,%s{opacity:0;transform:translateY(64px) scale(.7) rotate(-8deg)}%s,100%%{opacity:1;transform:none}}" % (p(.75), p(1.65)),
    ]
    for i in range(len(ls)):
        a = 1.3 + .07 * i
        css.append(f".l{i}{{animation-name:l{i};animation-timing-function:cubic-bezier(.2,.8,.2,1)}}"
                   f"@keyframes l{i}{{0%,{p(a)}{{opacity:0;transform:translateY(26px)}}{p(a + .55)},100%{{opacity:1;transform:none}}}}")
    body = (f'<g class="all">\n<path class="t" d="{TRAY}"/>\n<rect class="s" {" ".join(f"{k}=\"{v}\"" for k, v in SQUARE.items())}/>\n'
            + "\n".join(f'<path class="l l{i}" d="{d}"/>' for i, (_, d) in enumerate(ls)) + "\n</g>")
    return svg(vb, "<style>" + "".join(css) + "</style>\n" + body, "Unpack")


def write(name, text):
    (HERE / name).write_text(text)


def main():
    # wordmark origin: right of the tray (tray ends x=216) with a 72 gap
    X0 = 288
    ls, end = letters(X0)
    wm = lambda ink: "\n".join(f'<path fill="{ink}" d="{d}"/>' for _, d in ls)
    vb_lock = (16, 24, end + 8, 264)  # 24 unit margin all round

    write("unpack-symbol.svg", svg((0, 0, 256, 256), sym(INK, GREEN), "Unpack symbol"))
    write("unpack-symbol-dark.svg", svg((0, 0, 256, 256), sym(INK_DK, GREEN_DK), "Unpack symbol (for dark backgrounds)"))
    write("unpack-symbol-small.svg", svg((0, 0, 256, 256), sym(INK, GREEN, True), "Unpack symbol, small-size cut"))
    write("unpack-symbol-black.svg", svg((0, 0, 256, 256), sym("#000", "#000"), "Unpack symbol, black"))
    write("unpack-symbol-white.svg", svg((0, 0, 256, 256), sym("#fff", "#fff"), "Unpack symbol, white"))
    write("unpack-lockup.svg", svg(vb_lock, sym(INK, GREEN) + "\n" + wm(INK), "Unpack"))
    write("unpack-lockup-dark.svg", svg(vb_lock, sym(INK_DK, GREEN_DK) + "\n" + wm(INK_DK), "Unpack (for dark backgrounds)"))

    write("unpack-lockup-animated.svg", animated(ls, vb_lock))

    # app icon tile: dark squircle, light tray, green square; symbol scaled to ~58% and centred
    sc = 3.1
    ox = oy = 512 - 128 * sc
    tile = (f'<rect width="1024" height="1024" rx="228" fill="#161b22"/>\n'
            f'<g transform="translate({fmt(ox)} {fmt(oy)}) scale({sc})">\n{sym(INK_DK, GREEN_DK)}\n</g>')
    write("app-icon.svg", svg((0, 0, 1024, 1024), tile, "Unpack app icon"))

    # module for the React component
    names = ", ".join(f'"{d}"' for _, d in ls)
    TS_OUT.write_text(
        "// Generated by assets/brand/build.py. Do not edit.\n"
        f"export const TRAY = \"{TRAY}\";\n"
        f"export const TRAY_S = \"{TRAY_S}\";\n"
        f"export const LETTERS: string[] = [{names}];\n"
        f"export const LOCKUP_VIEWBOX = \"{' '.join(fmt(v) for v in vb_lock)}\";\n"
    )
    print("lockup viewBox", vb_lock)


if __name__ == "__main__":
    main()
