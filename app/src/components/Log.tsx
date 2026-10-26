import { useEffect, useRef, useState } from "react";
import { ChevronRight, Loader2 } from "lucide-react";
import type { LogLine } from "../api";

const color: Record<string, string> = { out: "text-ink", err: "text-warn", info: "text-info", warn: "text-warn" };

/** Collapsible "Output" panel for streamed command output, like VS Code's bottom panel. */
export function Log({ lines, running = false }: { lines: LogLine[]; running?: boolean }) {
  const [open, setOpen] = useState(true);
  const end = useRef<HTMLDivElement>(null);
  // Braces matter: newer engines return a Promise from scrollIntoView, and React treats a returned value as cleanup.
  useEffect(() => { end.current?.scrollIntoView({ block: "end" }); }, [lines.length]);
  return (
    <div className="enter overflow-hidden rounded-md border border-line">
      <button onClick={() => setOpen((o) => !o)} aria-expanded={open}
        className="flex w-full cursor-pointer items-center gap-1.5 bg-side px-2 py-1.5 text-left hover:bg-hover">
        <ChevronRight className={`h-4 w-4 text-dim transition-transform duration-150 ${open ? "rotate-90" : ""}`} aria-hidden />
        <span className="label">Output</span>
        {running && <Loader2 className="h-3.5 w-3.5 animate-spin text-dim" aria-hidden />}
        <span className="ml-auto text-[11px] text-dim">{lines.length} lines</span>
      </button>
      {open && (
        <div className="h-56 overflow-auto border-t border-line bg-term px-3 py-2 font-mono text-xs leading-relaxed" role="log">
          {/* Step markers drive the step list; they are noise here. */}
          {lines.filter((l) => !(l.stream === "info" && /^== .* ==$/.test(l.line))).map((l, i) => (
            <div key={i} className={`whitespace-pre-wrap [overflow-wrap:anywhere] ${color[l.stream] ?? "text-ink"}`}>{l.line || " "}</div>
          ))}
          <div ref={end} />
        </div>
      )}
    </div>
  );
}
