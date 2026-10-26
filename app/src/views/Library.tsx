import { useCallback, useEffect, useMemo, useState } from "react";
import { Loader2, RefreshCw, Search, Trash2 } from "lucide-react";
import { api, errText, when, type App, type LogLine, type SysPkg } from "../api";
import { Log } from "../components/Log";

type Tab = "tracked" | "system";

export default function Library({ version }: { version: number }) {
  const [tab, setTab] = useState<Tab>("tracked");
  const [apps, setApps] = useState<App[]>([]);
  const [sys, setSys] = useState<SysPkg[] | null>(null);
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [log, setLog] = useState<LogLine[]>([]);

  const loadApps = useCallback(() => api.apps().then(setApps).catch((e) => setMsg(errText(e))), []);
  const loadSys = useCallback(() => { setSys(null); api.scan().then(setSys).catch((e) => { setMsg(errText(e)); setSys([]); }); }, []);

  useEffect(() => { void loadApps(); }, [loadApps, version]);
  useEffect(() => { if (tab === "system" && sys === null) loadSys(); }, [tab, sys, loadSys]);
  useEffect(() => {
    const un = api.onLog((l) => { if (l.plan_id === "uninstall") setLog((x) => [...x, l]); });
    return () => { void un.then((f) => f()); };
  }, []);

  const remove = async (key: string, fn: () => Promise<{ ok: boolean; error: string | null }>) => {
    setBusy(key); setConfirm(null); setMsg(null); setLog([]);
    try {
      const r = await fn();
      if (!r.ok) setMsg(r.error ?? "Could not remove");
      await loadApps(); if (tab === "system") loadSys();
    } catch (e) { setMsg(errText(e)); }
    setBusy(null);
  };

  const ql = q.toLowerCase();
  const tracked = useMemo(() => apps.filter((a) => a.name.toLowerCase().includes(ql)), [apps, ql]);
  const system = useMemo(() => (sys ?? []).filter((p) => p.name.toLowerCase().includes(ql)), [sys, ql]);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <header className="flex items-end justify-between gap-4">
        <div><h1 className="text-3xl font-bold tracking-tight">Library</h1><p className="mt-1 text-dim">Everything you installed, in one place.</p></div>
        <button className="btn" onClick={() => { void loadApps(); if (tab === "system") loadSys(); }}><RefreshCw className="h-4 w-4" /> Refresh</button>
      </header>
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex rounded-xl border border-line bg-panel p-1">
          {(["tracked", "system"] as Tab[]).map((t) => (
            <button key={t} onClick={() => setTab(t)} className={`rounded-lg px-4 py-1.5 text-sm font-medium transition ${tab === t ? "bg-gradient-to-r from-brand to-brand2 text-white" : "text-dim hover:text-ink"}`}>
              {t === "tracked" ? `Installed with Unpack (${apps.filter((a) => a.status === "installed").length})` : "On this computer"}
            </button>
          ))}
        </div>
        <div className="relative min-w-48 flex-1"><Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-dim" /><input className="input pl-9!" placeholder="Search" value={q} onChange={(e) => setQ(e.target.value)} /></div>
      </div>
      {msg && <div className="whitespace-pre-wrap rounded-xl border border-rose-700/50 bg-rose-950/40 p-3 text-sm text-rose-200">{msg}</div>}
      {busy && <Log lines={log} />}

      <div className="glass divide-y divide-line overflow-hidden">
        {tab === "tracked" && tracked.map((a) => (
          <Row key={a.id} title={a.name} sub={`${a.version ?? ""} ${a.kind} · ${when(a.installed_at)}`.trim()} status={a.status}
            how={a.uninstall.type === "manager" ? `via ${a.uninstall.manager}` : a.uninstall.type === "files" ? `${a.uninstall.paths.length} files tracked` : a.uninstall.type === "manual" ? "remove manually" : ""}
            action={a.status === "installed" && a.uninstall.type !== "manual" && (
              <Remove id={`a${a.id}`} busy={busy} confirm={confirm} setConfirm={setConfirm} run={() => remove(`a${a.id}`, () => api.uninstall(a.id))} />)} />
        ))}
        {tab === "tracked" && tracked.length === 0 && <Empty>Nothing here yet. Install something and it will show up.</Empty>}
        {tab === "system" && sys === null && <Empty><Loader2 className="mx-auto h-5 w-5 animate-spin" /></Empty>}
        {tab === "system" && system.map((p) => (
          <Row key={`${p.manager}:${p.name}`} title={p.name} sub={p.version} status={p.manager} how=""
            action={<Remove id={`${p.manager}:${p.name}`} busy={busy} confirm={confirm} setConfirm={setConfirm} run={() => remove(`${p.manager}:${p.name}`, () => api.uninstallSystem(p.manager, p.name))} />} />
        ))}
        {tab === "system" && sys !== null && system.length === 0 && <Empty>No packages found.</Empty>}
      </div>
    </div>
  );
}

function Row({ title, sub, status, how, action }: { title: string; sub: string; status: string; how: string; action: React.ReactNode }) {
  return (
    <div className="flex items-center gap-4 px-5 py-3">
      <div className="min-w-0 flex-1"><div className="truncate font-medium">{title}</div><div className="truncate text-xs text-dim">{sub}{how && ` · ${how}`}</div></div>
      <span className={`chip ${status === "installed" ? "chip-ok" : ""}`}>{status}</span>
      {action}
    </div>
  );
}

function Remove({ id, busy, confirm, setConfirm, run }: { id: string; busy: string | null; confirm: string | null; setConfirm: (s: string | null) => void; run: () => void }) {
  if (busy === id) return <Loader2 className="h-4 w-4 animate-spin" />;
  if (confirm === id) return <span className="flex gap-1"><button className="btn btn-danger" onClick={run}>Really remove</button><button className="btn" onClick={() => setConfirm(null)}>No</button></span>;
  return <button className="btn" disabled={busy !== null} onClick={() => setConfirm(id)}><Trash2 className="h-4 w-4" /> Remove</button>;
}

const Empty = ({ children }: { children: React.ReactNode }) => <div className="px-5 py-10 text-center text-dim">{children}</div>;
