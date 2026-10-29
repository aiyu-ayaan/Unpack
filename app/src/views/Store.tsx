import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, Check, Loader2, Package, Search, Shield, XCircle } from "lucide-react";
import { api, errText, type App, type Detail, type Entry } from "../api";
import type { Incoming } from "../App";
import { Banner, Empty, PageHeader, Progress } from "../components/ui";

const SOURCE: Record<string, string> = { flathub: "Flathub", snap: "Snap Store", apt: "Debian/Ubuntu repos", dnf: "Fedora repos", zypper: "openSUSE repos", pacman: "Arch repos", apk: "Alpine repos", xbps: "Void repos", emerge: "Gentoo repos" };

/** Flathub category names; the backend adds the matching Snap Store section. */
const CATEGORIES: [string, string][] = [
  ["AudioVideo", "Audio & Video"], ["Development", "Development"], ["Education", "Education"], ["Game", "Games"], ["Graphics", "Graphics"],
  ["Network", "Internet"], ["Office", "Office"], ["Science", "Science"], ["System", "System"], ["Utility", "Utilities"],
];

type Mode = { kind: "popular" } | { kind: "search"; q: string } | { kind: "category"; id: string; label: string };

const sourceName = (s: string) => SOURCE[s] ?? (s.startsWith("flatpak:") ? `Flatpak (${s.slice(8)})` : s);

const mb = (b: number) => `${(b / 1048576).toFixed(0)} MB`;

export default function Store({ version, onReview }: { version: number; onReview: (i: Incoming) => void }) {
  const [q, setQ] = useState("");
  const [mode, setMode] = useState<Mode>({ kind: "popular" });
  const [items, setItems] = useState<Entry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState("all");
  const [apps, setApps] = useState<App[]>([]);
  const [open, setOpen] = useState<Entry | null>(null);

  useEffect(() => {
    setErr(null); setItems(null); setFilter("all"); setOpen(null);
    const load = mode.kind === "search" ? api.storeSearch(mode.q) : mode.kind === "category" ? api.storeCategory(mode.id) : api.storePopular();
    load.then(setItems).catch((e) => { setErr(errText(e)); setItems([]); });
  }, [mode]);
  useEffect(() => { api.apps().then(setApps).catch(() => {}); }, [version]);

  const installed = useMemo(() => new Set(apps.flatMap((a) => a.status === "installed" && a.uninstall.type === "manager" ? [a.uninstall.ident] : [])), [apps]);
  const sources = useMemo(() => [...new Set((items ?? []).map((i) => i.source))], [items]);
  const list = (items ?? []).filter((i) => filter === "all" || i.source === filter);

  const install = async (e: Entry) => {
    setBusy(`${e.source}:${e.id}`); setErr(null);
    try { onReview({ plan: await api.storePlan(e.source, e.id) }); } catch (x) { setErr(errText(x)); }
    setBusy(null);
  };

  const sub = mode.kind === "search" ? `Results for “${mode.q}”` : mode.kind === "category" ? `${mode.label} on Flathub and the Snap Store` : "Popular apps on Flathub, the Snap Store and your distro. Search to see more.";
  const action = (e: Entry) => installed.has(e.id)
    ? <span className="chip chip-ok"><Check className="h-3 w-3" aria-hidden /> Installed</span>
    : <button className="btn" disabled={busy !== null} onClick={() => void install(e)} aria-label={`Install ${e.name}`}>
        {busy === `${e.source}:${e.id}` && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />} Install</button>;

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-5">
      <PageHeader title="Store" sub={sub} />
      <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); setMode(q.trim() ? { kind: "search", q: q.trim() } : { kind: "popular" }); }}>
        <label className="relative flex-1">
          <span className="sr-only">Search apps</span>
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-dim" aria-hidden />
          <input className="input pl-8!" placeholder="Search apps, e.g. vlc, gimp, firefox" value={q} onChange={(e) => setQ(e.target.value)} spellCheck={false} />
        </label>
        <button className="btn btn-primary" disabled={items === null}>Search</button>
      </form>
      <div role="group" aria-label="Category" className="flex flex-wrap gap-1">
        <button aria-pressed={mode.kind === "popular"} onClick={() => { setQ(""); setMode({ kind: "popular" }); }} className={`btn ${mode.kind === "popular" ? "btn-primary" : ""}`}>Popular</button>
        {CATEGORIES.map(([id, label]) => {
          const on = mode.kind === "category" && mode.id === id;
          return <button key={id} aria-pressed={on} onClick={() => setMode({ kind: "category", id, label })} className={`btn ${on ? "btn-primary" : ""}`}>{label}</button>;
        })}
      </div>
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      {open ? <AppDetail e={open} action={action(open)} onBack={() => setOpen(null)} /> : <>
        {sources.length > 1 && (
          <div role="group" aria-label="Source" className="flex flex-wrap items-center gap-1">
            <span className="mr-1 text-xs text-dim">From</span>
            {["all", ...sources].map((s) => <button key={s} aria-pressed={filter === s} onClick={() => setFilter(s)} className={`chip cursor-pointer ${filter === s ? "chip-ok" : ""}`}>{s === "all" ? "All" : sourceName(s)}</button>)}
          </div>
        )}
        {items === null && <div className="flex flex-col gap-3 py-6"><Progress value={null} label="Loading apps" /><div className="text-center text-dim">Loading apps…</div></div>}
        {items !== null && list.length === 0 && !err && <div className="card"><Empty>No apps found. Try another name.</Empty></div>}
        <div className="stagger grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3">
          {list.map((e) => (
            <article key={`${e.source}:${e.id}`} className="card flex flex-col gap-3 p-4">
              <button className="flex cursor-pointer items-start gap-3 text-left" onClick={() => setOpen(e)} title="Details, screenshots and permissions">
                <Icon e={e} />
                <span className="min-w-0">
                  <span className="block truncate font-medium text-strong hover:underline">{e.name}</span>
                  <span className="line-clamp-2 text-xs text-dim">{e.summary || e.id}</span>
                </span>
              </button>
              <div className="mt-auto flex items-center justify-between gap-2">
                <span className="chip truncate" title={e.id}>{sourceName(e.source)}</span>
                {action(e)}
              </div>
            </article>
          ))}
        </div>
      </>}
    </div>
  );
}

const Icon = ({ e, big = false }: { e: Entry; big?: boolean }) => {
  const size = big ? "h-16 w-16" : "h-11 w-11";
  return e.icon
    ? <img src={e.icon} alt="" width={big ? 64 : 44} height={big ? 64 : 44} loading="lazy" className={`${size} shrink-0 rounded-md object-contain`} />
    : <span className={`grid ${size} shrink-0 place-items-center rounded-md bg-raised`}><Package className="h-5 w-5 text-dim" strokeWidth={1.5} aria-hidden /></span>;
};

/** One app before installing it: what it is, what it looks like, and what it may reach. */
function AppDetail({ e, action, onBack }: { e: Entry; action: React.ReactNode; onBack: () => void }) {
  const [d, setD] = useState<Detail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.storeDetail(e.source, e.id).then(setD).catch((x) => setErr(errText(x))); }, [e]);
  const facts = d ? [d.developer && `By ${d.developer}`, d.license, d.installed_size && `${mb(d.installed_size)} installed`, d.download_size && `${mb(d.download_size)} download`].filter(Boolean) as string[] : [];

  return (
    <article className="card enter flex flex-col gap-5 p-5">
      <button className="btn btn-ghost self-start" onClick={onBack}><ArrowLeft className="h-4 w-4" aria-hidden /> Back</button>
      <div className="flex items-start gap-4">
        <Icon e={e} big />
        <div className="min-w-0 flex-1">
          <h2 className="text-lg font-semibold text-strong">{e.name}</h2>
          <p className="text-dim">{e.summary}</p>
          <div className="mt-2 flex flex-wrap gap-1.5"><span className="chip">{sourceName(e.source)}</span>{facts.map((f) => <span key={f} className="chip">{f}</span>)}</div>
        </div>
        {action}
      </div>
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      {!d && !err && <Progress value={null} label="Loading details" />}
      {d && d.screenshots.length > 0 && (
        <div className="flex gap-3 overflow-x-auto pb-2" aria-label="Screenshots">
          {d.screenshots.map((src) => <img key={src} src={src} alt={`${e.name} screenshot`} loading="lazy" className="h-56 w-auto shrink-0 rounded-md border border-line" />)}
        </div>
      )}
      {d && <p className="whitespace-pre-line text-sm text-ink">{d.description || e.summary || "No description."}</p>}
      {d && d.permissions.length > 0 && (
        <section>
          <h3 className="label mb-2">What it can reach</h3>
          <ul className="flex flex-col gap-1.5">
            {d.permissions.map((p) => <li key={p} className="flex items-start gap-2 text-sm"><Shield className="mt-0.5 h-4 w-4 shrink-0 text-dim" aria-hidden /> {p}</li>)}
          </ul>
        </section>
      )}
    </article>
  );
}
