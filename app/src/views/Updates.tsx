import { useState } from "react";
import { ArrowRight, BellOff, CheckCircle2, Loader2, RefreshCw, SkipForward, XCircle } from "lucide-react";
import { api, errText, type Plan, type Upgrade } from "../api";
import type { Incoming } from "../App";
import { Banner, Empty, PageHeader, Progress } from "../components/ui";

const MANAGER: Record<string, string> = { download: "New release", flatpak: "Flatpak", snap: "Snap" };
const key = (u: Upgrade) => `${u.manager}:${u.user}:${u.name}`;

export default function Updates({ list, err, reload, onReview }: { list: Upgrade[] | null; err: string | null; reload: () => void; onReview: (i: Incoming) => void }) {
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const review = async (id: string, make: () => Promise<Plan>) => {
    setBusy(id); setMsg(null);
    try { onReview({ plan: await make() }); } catch (e) { setMsg(errText(e)); }
    setBusy(null);
  };
  const packages = (us: Upgrade[]) => review(us.length === 1 ? key(us[0]) : "all", () => api.upgradePlan(us.map(({ manager, name, user }) => ({ manager, name, user }))));
  const one = (u: Upgrade) => u.manager === "download" && u.app_id !== null ? review(key(u), () => api.updateAppPlan(u.app_id!)) : packages([u]);

  const hold = async (u: Upgrade, version: string | null) => {
    setMsg(null);
    try { await api.holdUpdate(u.manager, u.name, version); reload(); } catch (e) { setMsg(errText(e)); }
  };
  const unhold = async (u: Upgrade) => {
    setMsg(null);
    try { await api.unholdUpdate(u.manager, u.name); reload(); } catch (e) { setMsg(errText(e)); }
  };

  const offered = (list ?? []).filter((u) => !u.held);
  const mine = offered.filter((u) => u.app_id !== null);
  const system = offered.filter((u) => u.app_id === null);
  const held = (list ?? []).filter((u) => u.held);
  const batch = offered.filter((u) => u.manager !== "download");

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <PageHeader title="Updates" sub="Newer versions of your apps and system packages. Updates are reviewed before they run.">
        <div className="flex gap-2">
          <button className="btn" disabled={list === null} onClick={reload}><RefreshCw className="h-4 w-4" aria-hidden /> Check again</button>
          <button className="btn btn-primary" disabled={!batch.length || busy !== null} onClick={() => void packages(batch)}>Update all ({batch.length})</button>
        </div>
      </PageHeader>
      {(err || msg) && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{msg ?? err}</Banner>}
      {list === null && <div className="flex flex-col gap-3 py-6"><Progress value={null} label="Checking for updates" /><div className="text-center text-dim">Asking your package managers and GitHub…</div></div>}
      {list !== null && offered.length === 0 && !err && <div className="card"><Empty><CheckCircle2 className="mx-auto mb-2 h-6 w-6 text-accent-ink" aria-hidden />Everything is up to date.</Empty></div>}
      {([["Installed with Unpack", mine], ["Everything else on this computer", system]] as const).map(([title, rows]) => rows.length > 0 && (
        <section key={title} className="card overflow-hidden">
          <h2 className="label border-b border-line px-4 py-2.5">{title} ({rows.length})</h2>
          <div className="stagger divide-y divide-line">
            {rows.map((u) => (
              <div key={key(u)} className="flex items-center gap-4 px-4 py-2 transition-colors hover:bg-hover">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-strong">{u.name}</div>
                  <div className="flex items-center gap-1 truncate text-xs text-dim">{u.current || "installed"} <ArrowRight className="h-3 w-3" aria-label="to" /> {u.latest || "newer"}</div>
                </div>
                <span className="chip">{MANAGER[u.manager] ?? u.manager}</span>
                <button className="btn btn-ghost px-1.5!" disabled={busy !== null} onClick={() => void hold(u, u.latest || null)} title="Skip this version" aria-label={`Skip ${u.name} ${u.latest}`}><SkipForward className="h-4 w-4" aria-hidden /></button>
                <button className="btn btn-ghost px-1.5!" disabled={busy !== null} onClick={() => void hold(u, null)} title="Don't update this app" aria-label={`Don't update ${u.name}`}><BellOff className="h-4 w-4" aria-hidden /></button>
                <button className="btn" disabled={busy !== null} onClick={() => void one(u)}>{busy === key(u) && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />} Update</button>
              </div>
            ))}
          </div>
        </section>
      ))}
      {held.length > 0 && (
        <section className="card overflow-hidden">
          <h2 className="label border-b border-line px-4 py-2.5">Not updated, as you asked ({held.length})</h2>
          <div className="divide-y divide-line">
            {held.map((u) => (
              <div key={key(u)} className="flex items-center gap-4 px-4 py-2">
                <div className="min-w-0 flex-1"><div className="truncate text-strong">{u.name}</div><div className="truncate text-xs text-dim">{u.latest || "newer"} available</div></div>
                <span className="chip">{MANAGER[u.manager] ?? u.manager}</span>
                <button className="btn btn-ghost" onClick={() => void unhold(u)}>Allow updates</button>
              </div>
            ))}
          </div>
        </section>
      )}
      {list !== null && list.length > 0 && <p className="text-xs text-dim">Package lists come from each source's last refresh; updating refreshes them first where the package manager allows it. New releases of apps from GitHub are checked against the project's latest release.</p>}
    </div>
  );
}
