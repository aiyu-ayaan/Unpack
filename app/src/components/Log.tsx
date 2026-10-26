import { useEffect, useRef } from "react";
import type { LogLine } from "../api";

const color: Record<string, string> = { out: "text-ink", err: "text-amber-300", info: "text-brand2", warn: "text-amber-300" };

export function Log({ lines }: { lines: LogLine[] }) {
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => end.current?.scrollIntoView({ block: "end" }), [lines.length]);
  return (
    <div className="h-64 overflow-auto rounded-xl border border-line bg-[#07070f] p-3 font-mono text-xs leading-relaxed">
      {lines.map((l, i) => (
        <div key={i} className={color[l.stream] ?? "text-ink"}>{l.line || " "}</div>
      ))}
      <div ref={end} />
    </div>
  );
}
