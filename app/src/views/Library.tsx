import { useCallback, useEffect, useMemo, useState } from "react";
import { FolderX, Loader2, RefreshCw, Search, Trash2, Undo2, Wrench, XCircle } from "lucide-react";
import { api, errText, when, type App, type LogLine, type Plan, type SysPkg } from "../api";
import type { Incoming, Task } from "../App";
import { Log } from "../components/Log";
import { Banner, Empty, PageHeader, Progress } from "../components/ui";

type Tab = "tracked" | "system";

export default function Library({ version, onTask, onReview }: { version: number; onTask: (t: Task) => void; onReview: (i: Incoming) => void }) {
  const [tab, setTab] = useState<Tab>("tracked");
  const [apps, setApps] = useState<App[]>([]);
  const [sys, setSys] = useState<SysPkg[] | null>(null);
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [log, setLog] = useState<LogLine[]>([]);
  /** Folders a just-removed app left behind, offered for removal. */
  const [left, setLeft] = useState<{ id: number; name: string; paths: string[] } | null>(null);

  const loadApps = useCallback(() => api.apps().then(setApps).catch((e) => setMsg(errText(e))), []);
  const loadSys = useCallback(() => { setSys(null); api.scan().then(setSys).catch((e) => { setMsg(errText(e)); setSys([]); }); }, []);

  useEffect(() => { void loadApps(); }, [loadApps, version]);
  useEffect(() => { if (tab === "system" && sys === null) loadSys(); }, [tab, sys, loadSys]);
  useEffect(() => {
    const un = api.onLog((l) => { if (l.plan_id === "uninstall") setLog((x) => [...x, l]); });
    return () => { void un.then((f) => f()); };
  }, []);

  const remove = async (key: string, name: string, fn: () => Promise<{ ok: boolean; error: string | null; app_id: number | null }>, tracked = false) => {
    setBusy(key); setConfirm(null); setMsg(null); setLog([]); setLeft(null);
    onTask({ label: `Removing ${name}`, progress: null });
    try {
      const r = await fn();
      if (!r.ok) setMsg(r.error ?? "Could not remove");
      if (r.ok && tracked && r.app_id !== null) {
        const id = r.app_id;
        const paths = await api.leftovers(id).catch(() => []);
        if (paths.length) setLeft({ id, name, paths });
      }
      await loadApps(); if (tab === "system") loadSys();
    } catch (e) { setMsg(errText(e)); }
    setBusy(null); onTask(null);
  };

  const purge = async () => {
    if (!left) return;
    setBusy("leftovers"); setMsg(null); setLog([]);
    onTask({ label: `Cleaning up after ${left.name}`, progress: null });
    try { await api.removeLeftovers(left.id); setLeft(null); } catch (e) { setMsg(errText(e)); }
    setBusy(null); onTask(null);
  };

  const review = async (key: string, make: () => Promise<Plan>) => {
    setBusy(key); setMsg(null);
    try { onReview({ plan: await make() }); } catch (e) { setMsg(errText(e)); }
    setBusy(null);
  };
  const fix = (a: App) => review(`fix${a.id}`, () => api.fixPlan(a.id));
  const rollback = (a: App) => review(`back${a.id}`, () => api.rollbackPlan(a.id));
  /** Apps Unpack updated, which can go back to the version before. */
  const updated = useMemo(() => new Set(apps.flatMap((a) => a.replaced_by === null ? [] : [a.replaced_by])), [apps]);

  const ql = q.toLowerCase();
  const tracked = useMemo(() => apps.filter((a) => a.status === "installed" && a.name.toLowerCase().includes(ql)), [apps, ql]);
  const system = useMemo(() => (sys ?? []).filter((p) => p.name.toLowerCase().includes(ql)), [sys, ql]);
  /** The same app installed with more than one package manager: `vlc` from apt and `org.videolan.VLC` from Flathub. */
  const twins = useMemo(() => {
    const by = new Map<string, Set<string>>();
    for (const p of sys ?? []) { const k = appKey(p); by.set(k, (by.get(k) ?? new Set()).add(p.manager)); }
    return by;
  }, [sys]);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <PageHeader title="Library" sub="Everything you installed, in one place.">
        <button className="btn" onClick={() => { void loadApps(); if (tab === "system") loadSys(); }}><RefreshCw className="h-4 w-4" aria-hidden /> Refresh</button>
      </PageHeader>
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex gap-4 border-b border-line" role="tablist">
          {(["tracked", "system"] as Tab[]).map((t) => (
            <button key={t} role="tab" aria-selected={tab === t} onClick={() => setTab(t)}
              className={`-mb-px cursor-pointer border-b-2 px-1 pb-2 transition-colors duration-150 ${tab === t ? "border-accent-ink text-strong" : "border-transparent text-dim hover:text-ink"}`}>
              {t === "tracked" ? `Installed with Unpack (${apps.filter((a) => a.status === "installed").length})` : "Everything on this computer"}
            </button>
          ))}
        </div>
        <label className="relative ml-auto w-64">
          <span className="sr-only">Search packages</span>
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-dim" aria-hidden />
          <input className="input pl-8!" placeholder="Search" value={q} onChange={(e) => setQ(e.target.value)} spellCheck={false} />
        </label>
      </div>
      {msg && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{msg}</Banner>}
      {left && (
        <Banner tone="info" icon={<FolderX className="h-4 w-4" />}>
          <div>{left.name} is removed. It left its settings, cache or data here:</div>
          <ul className="my-1.5 font-mono text-xs">{left.paths.map((p) => <li key={p}>{p}</li>)}</ul>
          <div className="mt-2 flex gap-2">
            <button className="btn btn-danger" disabled={busy !== null} onClick={() => void purge()}>Remove these too</button>
            <button className="btn" onClick={() => setLeft(null)}>Keep them</button>
          </div>
        </Banner>
      )}
      {busy && !busy.startsWith("fix") && !busy.startsWith("back") && <><Progress value={null} label="Removing" /><Log lines={log} running /></>}

      <div className="card divide-y divide-line overflow-hidden">
        {tab === "tracked" && <div className="stagger divide-y divide-line">{tracked.map((a) => (
          <Row key={a.id} title={a.name} sub={`${a.version ?? ""} ${a.kind} · ${when(a.installed_at)}`.trim()} status={a.status}
            how={a.uninstall.type === "manager" ? `via ${a.uninstall.manager}` : a.uninstall.type === "files" ? `${a.uninstall.paths.length} files tracked` : a.uninstall.type === "manual" ? "remove manually" : a.uninstall.type === "container" ? `in container ${a.uninstall.container}` : ""}
            action={a.status === "installed" && <span className="flex gap-1">
              {updated.has(a.id) && <button className="btn btn-ghost" disabled={busy !== null} onClick={() => void rollback(a)} title="Go back to the version before the last update">
                {busy === `back${a.id}` ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <Undo2 className="h-4 w-4" aria-hidden />} Undo update</button>}
              <button className="btn btn-ghost" disabled={busy !== null} onClick={() => void fix(a)} title="Find and repair what keeps it from starting">
                {busy === `fix${a.id}` ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <Wrench className="h-4 w-4" aria-hidden />} Fix</button>
              {a.uninstall.type !== "manual" && <Remove id={`a${a.id}`} busy={busy} confirm={confirm} setConfirm={setConfirm} run={() => remove(`a${a.id}`, a.name, () => api.uninstall(a.id), true)} />}
            </span>} />
        ))}</div>}
        {tab === "tracked" && tracked.length === 0 && <Empty>Nothing here yet. Install something and it will show up.</Empty>}
        {tab === "system" && sys === null && <div className="flex flex-col gap-3 px-5 py-8"><Progress value={null} label="Scanning packages" /><div className="text-center text-dim">Looking for installed packages…</div></div>}
        {tab === "system" && system.map((p) => (
          <Row key={`${p.manager}:${p.name}`} title={p.name} sub={p.version} status={p.manager}
            how={[...(twins.get(appKey(p)) ?? [])].filter((m) => m !== p.manager).map((m) => `also installed with ${m}`).join(", ")}
            action={<Remove id={`${p.manager}:${p.name}`} busy={busy} confirm={confirm} setConfirm={setConfirm} run={() => remove(`${p.manager}:${p.name}`, p.name, () => api.uninstallSystem(p.manager, p.name))} />} />
        ))}
        {tab === "system" && sys !== null && system.length === 0 && <Empty>No packages found.</Empty>}
      </div>
    </div>
  );
}

const appKey = (p: SysPkg) => (p.manager === "flatpak" ? p.name.split(".").pop() ?? p.name : p.name).toLowerCase();

function Row({ title, sub, status, how, action }: { title: string; sub: string; status: string; how: string; action: React.ReactNode }) {
  return (
    <div className="group flex items-center gap-4 px-4 py-2 transition-colors hover:bg-hover">
      <div className="min-w-0 flex-1"><div className="truncate text-strong">{title}</div><div className="truncate text-xs text-dim">{sub}{how && ` · ${how}`}</div></div>
      <span className={`chip ${status === "installed" ? "chip-ok" : ""}`}>{status}</span>
      {action}
    </div>
  );
}

function Remove({ id, busy, confirm, setConfirm, run }: { id: string; busy: string | null; confirm: string | null; setConfirm: (s: string | null) => void; run: () => void }) {
  if (busy === id) return <span className="text-xs text-dim">Removing…</span>;
  if (confirm === id) return <span className="enter flex gap-1"><button className="btn btn-danger" onClick={run}>Remove</button><button className="btn" onClick={() => setConfirm(null)}>Cancel</button></span>;
  return <button className="btn btn-ghost" disabled={busy !== null} onClick={() => setConfirm(id)}><Trash2 className="h-4 w-4" aria-hidden /> Remove</button>;
}
