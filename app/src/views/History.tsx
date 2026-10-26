import { useEffect, useState } from "react";
import { api, errText, when, type Event } from "../api";

export default function History({ version }: { version: number }) {
  const [ev, setEv] = useState<Event[]>([]);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.history().then(setEv).catch((e) => setErr(errText(e))); }, [version]);
  const tone: Record<string, string> = { error: "text-rose-300", warn: "text-amber-300", info: "text-ink" };
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <header><h1 className="text-3xl font-bold tracking-tight">History</h1><p className="mt-1 text-dim">What Unpack did and when.</p></header>
      {err && <div className="text-rose-300">{err}</div>}
      <div className="glass divide-y divide-line">
        {ev.map((e, i) => (
          <div key={i} className="flex gap-4 px-5 py-3 text-sm"><span className="w-40 shrink-0 text-dim">{when(e.ts)}</span><span className={tone[e.level] ?? ""}>{e.message}</span></div>
        ))}
        {ev.length === 0 && <div className="px-5 py-10 text-center text-dim">Nothing yet.</div>}
      </div>
    </div>
  );
}
