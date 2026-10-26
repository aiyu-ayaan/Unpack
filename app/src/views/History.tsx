import { useEffect, useState } from "react";
import { XCircle } from "lucide-react";
import { api, errText, when, type Event } from "../api";
import { Banner, Empty, PageHeader } from "../components/ui";

const tone: Record<string, string> = { error: "text-bad", warn: "text-warn", info: "text-ink" };
const tag: Record<string, string> = { error: "ERR", warn: "WRN", info: "INF" };

export default function History({ version }: { version: number }) {
  const [ev, setEv] = useState<Event[]>([]);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.history().then(setEv).catch((e) => setErr(errText(e))); }, [version]);
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <PageHeader title="history" sub="What Unpack did and when." />
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      <div className="card overflow-hidden bg-term font-mono text-xs">
        <div className="stagger">
          {ev.map((e, i) => (
            <div key={i} className="flex gap-3 border-b border-line/60 px-4 py-2 last:border-0 hover:bg-raised">
              <span className="w-40 shrink-0 text-dim">{when(e.ts)}</span>
              <span className={`w-8 shrink-0 font-semibold ${e.level === "info" ? "text-accent-ink" : tone[e.level]}`}>{tag[e.level] ?? e.level}</span>
              <span className={`min-w-0 [overflow-wrap:anywhere] ${tone[e.level] ?? ""}`}>{e.message}</span>
            </div>
          ))}
        </div>
        {ev.length === 0 && <Empty>no events yet</Empty>}
      </div>
    </div>
  );
}
