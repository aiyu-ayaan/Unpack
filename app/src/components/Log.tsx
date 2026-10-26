import { useEffect, useRef } from "react";
import { SquareTerminal } from "lucide-react";
import type { LogLine } from "../api";

const color: Record<string, string> = { out: "text-ink", err: "text-warn", info: "text-accent-ink", warn: "text-warn" };

/** Terminal-style panel for streamed command output. */
export function Log({ lines, running = false }: { lines: LogLine[]; running?: boolean }) {
  const end = useRef<HTMLDivElement>(null);
  // Braces matter: newer engines return a Promise from scrollIntoView, and React treats a returned value as cleanup.
  useEffect(() => { end.current?.scrollIntoView({ block: "end" }); }, [lines.length]);
  return (
    <div className="enter overflow-hidden rounded-md border border-line bg-term">
      <div className="flex items-center gap-2 border-b border-line px-3 py-1.5">
        <SquareTerminal className="h-3.5 w-3.5 text-accent-ink" aria-hidden />
        <span className="label">Terminal</span>
        <span className="ml-auto font-mono text-[10px] text-dim">{lines.length} lines</span>
      </div>
      <div className="h-64 overflow-auto p-3 font-mono text-xs leading-relaxed" role="log">
        {lines.map((l, i) => (
          <div key={i} className={`whitespace-pre-wrap [overflow-wrap:anywhere] ${color[l.stream] ?? "text-ink"}`}>{l.line || " "}</div>
        ))}
        {running && <div className="cursor text-dim" />}
        <div ref={end} />
      </div>
    </div>
  );
}
