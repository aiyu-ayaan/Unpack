import { useEffect, useMemo, useState } from "react";
import { Check, Loader2, Package, Search, XCircle } from "lucide-react";
import { api, errText, type App, type Entry } from "../api";
import type { Incoming } from "../App";
import { Banner, Empty, PageHeader, Progress } from "../components/ui";

const SOURCE: Record<string, string> = { flathub: "Flathub", snap: "Snap Store", apt: "Debian/Ubuntu repos", dnf: "Fedora repos", zypper: "openSUSE repos", pacman: "Arch repos", apk: "Alpine repos" };

export default function Store({ version, onReview }: { version: number; onReview: (i: Incoming) => void }) {
  const [q, setQ] = useState("");
  const [shown, setShown] = useState<string | null>(null);
  const [items, setItems] = useState<Entry[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [filter, setFilter] = useState("all");
  const [apps, setApps] = useState<App[]>([]);

  useEffect(() => { api.storePopular().then(setItems).catch((e) => { setErr(errText(e)); setItems([]); }); }, []);
  useEffect(() => { api.apps().then(setApps).catch(() => {}); }, [version]);

  const installed = useMemo(() => new Set(apps.flatMap((a) => a.status === "installed" && a.uninstall.type === "manager" ? [a.uninstall.ident] : [])), [apps]);
  const sources = useMemo(() => [...new Set((items ?? []).map((i) => i.source))], [items]);
  const list = (items ?? []).filter((i) => filter === "all" || i.source === filter);

  const search = (text: string) => {
    setErr(null); setItems(null); setFilter("all"); setShown(text.trim() || null);
    (text.trim() ? api.storeSearch(text) : api.storePopular()).then(setItems).catch((e) => { setErr(errText(e)); setItems([]); });
  };

  const install = async (e: Entry) => {
    setBusy(`${e.source}:${e.id}`); setErr(null);
    try { onReview({ plan: await api.storePlan(e.source, e.id) }); } catch (x) { setErr(errText(x)); }
    setBusy(null);
  };

  return (
    <div className="mx-auto flex max-w-5xl flex-col gap-5">
      <PageHeader title="Store" sub={shown ? `Results for “${shown}”` : "Popular apps on Flathub. Search to include your distro's own packages."} />
      <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); search(q); }}>
        <label className="relative flex-1">
          <span className="sr-only">Search apps</span>
          <Search className="absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-dim" aria-hidden />
          <input className="input pl-8!" placeholder="Search apps, e.g. vlc, gimp, firefox" value={q} onChange={(e) => setQ(e.target.value)} spellCheck={false} />
        </label>
        <button className="btn btn-primary" disabled={items === null}>Search</button>
        {shown && <button type="button" className="btn" onClick={() => { setQ(""); search(""); }}>Popular</button>}
      </form>
      {sources.length > 1 && (
        <div role="group" aria-label="Source" className="flex flex-wrap gap-1">
          {["all", ...sources].map((s) => <button key={s} aria-pressed={filter === s} onClick={() => setFilter(s)} className={`btn ${filter === s ? "btn-primary" : ""}`}>{s === "all" ? "All" : SOURCE[s] ?? s}</button>)}
        </div>
      )}
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      {items === null && <div className="flex flex-col gap-3 py-6"><Progress value={null} label="Loading apps" /><div className="text-center text-dim">Loading apps…</div></div>}
      {items !== null && list.length === 0 && !err && <div className="card"><Empty>No apps found. Try another name.</Empty></div>}
      <div className="stagger grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3">
        {list.map((e) => {
          const key = `${e.source}:${e.id}`;
          const have = installed.has(e.id);
          return (
            <article key={key} className="card flex flex-col gap-3 p-4">
              <div className="flex items-start gap-3">
                {e.icon ? <img src={e.icon} alt="" width={44} height={44} loading="lazy" className="h-11 w-11 shrink-0 rounded-md object-contain" />
                  : <div className="grid h-11 w-11 shrink-0 place-items-center rounded-md bg-raised"><Package className="h-5 w-5 text-dim" strokeWidth={1.5} aria-hidden /></div>}
                <div className="min-w-0">
                  <h2 className="truncate font-medium text-strong" title={e.name}>{e.name}</h2>
                  <p className="line-clamp-2 text-xs text-dim">{e.summary || e.id}</p>
                </div>
              </div>
              <div className="mt-auto flex items-center justify-between gap-2">
                <span className="chip truncate" title={e.id}>{SOURCE[e.source] ?? e.source}</span>
                {have ? <span className="chip chip-ok"><Check className="h-3 w-3" aria-hidden /> Installed</span>
                  : <button className="btn" disabled={busy !== null} onClick={() => void install(e)} aria-label={`Install ${e.name}`}>
                      {busy === key && <Loader2 className="h-4 w-4 animate-spin" aria-hidden />} Install</button>}
              </div>
            </article>
          );
        })}
      </div>
    </div>
  );
}
