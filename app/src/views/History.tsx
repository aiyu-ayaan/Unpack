import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, XCircle } from "lucide-react";
import { api, errText, when, type Event } from "../api";
import { Banner, Empty, PageHeader } from "../components/ui";

const ICON: Record<string, React.ReactNode> = {
  error: <XCircle className="h-4 w-4 text-bad" aria-label="Error" />,
  warn: <AlertTriangle className="h-4 w-4 text-warn" aria-label="Warning" />,
  info: <CheckCircle2 className="h-4 w-4 text-accent-ink" aria-label="Done" />,
};

export default function History({ version }: { version: number }) {
  const [ev, setEv] = useState<Event[]>([]);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.history().then(setEv).catch((e) => setErr(errText(e))); }, [version]);
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <PageHeader title="History" sub="What Unpack did and when." />
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      <div className="card overflow-hidden">
        <div className="stagger">
          {ev.map((e, i) => (
            <div key={i} className="flex items-start gap-3 border-b border-line px-4 py-2.5 last:border-0 hover:bg-hover">
              <span className="mt-0.5 shrink-0">{ICON[e.level] ?? ICON.info}</span>
              <span className="min-w-0 flex-1 text-strong [overflow-wrap:anywhere]">{e.message}</span>
              <span className="shrink-0 text-xs tabular-nums text-dim">{when(e.ts)}</span>
            </div>
          ))}
        </div>
        {ev.length === 0 && <Empty>Nothing yet. Installs and removals will show up here.</Empty>}
      </div>
    </div>
  );
}
