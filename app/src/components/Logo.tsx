import type { CSSProperties } from "react";
import { LETTERS, LOCKUP_VIEWBOX, TRAY, TRAY_S } from "./logoPaths";
import "./logo.css";

// Square sits at the same place in both cuts; see assets/brand/build.py.
const SQUARE = { x: 96, y: 48, width: 64, height: 64, rx: 10 };
const SQUARE_S = { x: 100, y: 32, width: 56, height: 56, rx: 8 };

/** The Unpack logo, themed from the CSS tokens. `small` is the heavier cut for 16-24 px. `animate` plays the intro once. */
export function Logo({ variant = "symbol", small = false, animate = false, className }: {
  variant?: "symbol" | "lockup";
  small?: boolean;
  animate?: boolean;
  className?: string;
}) {
  const lockup = variant === "lockup";
  const viewBox = lockup ? LOCKUP_VIEWBOX : small ? "36 28 184 200" : "36 44 184 168";
  const sq = small && !lockup ? SQUARE_S : SQUARE;
  return (
    <svg viewBox={viewBox} className={`${animate ? "lg-anim " : ""}${className ?? ""}`} aria-hidden>
      <path className="lg-tray" d={small && !lockup ? TRAY_S : TRAY} />
      <rect className="lg-sq" {...sq} />
      {lockup && LETTERS.map((d, i) => <path key={i} className="lg-let" d={d} style={{ "--i": i } as CSSProperties} />)}
    </svg>
  );
}
