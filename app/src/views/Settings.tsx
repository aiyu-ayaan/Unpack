import { useEffect, useState, type ReactNode } from "react";
import { ExternalLink, Code2, UserRound, Globe, Bug, RefreshCw, XCircle, CheckCircle2, Download } from "lucide-react";
import { api, errText, type Channel, type Imported, type Link } from "../api";
import type { Incoming } from "../App";
import { plainNotes, type Update } from "../update";
import { Banner, PageHeader, Progress } from "../components/ui";

const CHANNELS: { id: Channel; label: string; hint: string }[] = [
  { id: "stable", label: "Stable", hint: "Tested releases only" },
  { id: "beta", label: "Beta", hint: "Stable plus early previews" },
  { id: "alpha", label: "Alpha", hint: "Everything, newest first" },
];

const CREDITS: [string, string][] = [
  ["Tauri", "MIT / Apache-2.0"], ["React", "MIT"], ["Tailwind CSS", "MIT"], ["Vite", "MIT"], ["Lucide", "ISC"],
  ["Rust", "MIT / Apache-2.0"], ["rusqlite and SQLite", "MIT / Public domain"], ["ureq", "MIT / Apache-2.0"],
];

const Row = ({ k, children }: { k: string; children: ReactNode }) => (
  <div className="flex min-h-11 items-center justify-between gap-4 border-b border-line px-4 py-2 last:border-0">
    <span className="text-dim">{k}</span><span className="text-right text-strong">{children}</span>
  </div>
);

const LinkRow = ({ icon, title, sub, to }: { icon: ReactNode; title: string; sub: string; to: Link }) => (
  <button onClick={() => void api.openLink(to).catch((e) => alert(errText(e)))}
    className="flex min-h-12 w-full cursor-pointer items-center justify-between gap-3 border-b border-line px-4 py-2 text-left transition-colors last:border-0 hover:bg-hover">
    <span className="flex items-center gap-3"><span className="text-dim" aria-hidden>{icon}</span>
      <span><span className="block text-strong">{title}</span><span className="block text-xs text-dim">{sub}</span></span></span>
    <ExternalLink className="h-4 w-4 text-dim" aria-hidden />
  </button>
);

const Card = ({ title, children }: { title: string; children: ReactNode }) => (
  <section className="card overflow-hidden"><h2 className="label border-b border-line px-4 py-2.5">{title}</h2>{children}</section>
);

function FileHandler() {
  const [on, setOn] = useState<boolean | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.fileHandler().then(setOn).catch(() => setOn(false)); }, []);
  const toggle = async () => {
    setErr(null);
    try { await api.setFileHandler(!on); } catch (e) { setErr(errText(e)); }
    setOn(await api.fileHandler().catch(() => false));
  };
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
        <div><p className="text-strong">Open package files with Unpack</p><p className="text-xs text-dim">Double-click a .deb, .rpm, .AppImage, .flatpak or .snap to check it here first.</p></div>
        <button role="switch" aria-checked={!!on} disabled={on === null} onClick={() => void toggle()} className={`btn ${on ? "btn-primary" : ""}`}>{on ? "On" : "Off"}</button>
      </div>
      {err && <div className="border-t border-line p-4"><Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner></div>}
    </>
  );
}

function Transfer({ onReview }: { onReview: (i: Incoming) => void }) {
  const [saved, setSaved] = useState<string | null>(null);
  const [path, setPath] = useState("~/unpack-apps.json");
  const [list, setList] = useState<Imported[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const run = async (f: () => Promise<void>) => { setErr(null); try { await f(); } catch (e) { setErr(errText(e)); } };
  const bring = (i: Imported) => run(async () => {
    if (i.way.type === "store") onReview({ plan: await api.storePlan(i.way.source, i.way.id) });
    else if (i.way.type === "input") onReview({ input: i.way.input });
  });
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3">
        <div><p className="text-strong">Export your apps</p><p className="text-xs text-dim">Saves the list of apps installed with Unpack, to set up another computer.</p></div>
        <button className="btn" onClick={() => void run(async () => setSaved(await api.exportApps()))}>Export</button>
      </div>
      {saved && <p className="border-b border-line px-4 py-2 text-xs text-dim">Saved to <span className="font-mono">{saved}</span></p>}
      <form className="flex flex-wrap items-center gap-2 px-4 py-3" onSubmit={(e) => { e.preventDefault(); void run(async () => setList(await api.importApps(path))); }}>
        <label className="min-w-0 flex-1"><span className="sr-only">Exported list to import</span>
          <input className="input" value={path} onChange={(e) => setPath(e.target.value)} spellCheck={false} /></label>
        <button className="btn">Import</button>
      </form>
      {err && <div className="border-t border-line p-4"><Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner></div>}
      {list && (list.length === 0 ? <p className="border-t border-line px-4 py-3 text-dim">That list is empty.</p> : list.map((i, n) => (
        <div key={n} className="flex items-center gap-3 border-t border-line px-4 py-2">
          <div className="min-w-0 flex-1"><div className="truncate text-strong">{i.item.name}</div>
            <div className="truncate text-xs text-dim">{i.way.type === "store" ? `From ${i.way.source}` : i.way.type === "input" ? i.way.input : i.way.why}</div></div>
          {i.way.type !== "none" && <button className="btn" onClick={() => void bring(i)}>Install</button>}
        </div>
      )))}
    </>
  );
}

export default function Settings({ u, onReview }: { u: Update; onReview: (i: Incoming) => void }) {
  const { info, offer } = u;
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <PageHeader title="Settings" sub="Updates, files, your app list, and who made Unpack." />

      <Card title="Updates">
        <Row k="Installed version">{info?.version ?? "…"}</Row>
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-3">
          <div><p className="text-strong">Update channel</p><p className="text-xs text-dim">{CHANNELS.find((c) => c.id === info?.channel)?.hint}</p></div>
          <div role="group" aria-label="Update channel" className="flex gap-1">
            {CHANNELS.map((c) => (
              <button key={c.id} aria-pressed={info?.channel === c.id} disabled={!info || u.checking}
                onClick={() => void u.setChannel(c.id)} className={`btn ${info?.channel === c.id ? "btn-primary" : ""}`}>{c.label}</button>
            ))}
          </div>
        </div>
        <div className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
          <p className="flex items-center gap-2 text-strong">
            {u.checking ? <><RefreshCw className="h-4 w-4 animate-spin" aria-hidden /> Checking…</>
              : offer ? <><Download className="h-4 w-4 text-accent-ink" aria-hidden /> Unpack {offer.version} is available</>
              : u.checked ? <><CheckCircle2 className="h-4 w-4 text-accent-ink" aria-hidden /> You are up to date</>
              : "Not checked yet"}
          </p>
          {offer
            ? <button className="btn btn-primary" disabled={u.installing} onClick={() => void u.install()}>
                {u.installing ? "Downloading…" : info?.self_install ? "Update and restart" : "Download"}</button>
            : <button className="btn" disabled={u.checking} onClick={() => void u.check()}>Check now</button>}
        </div>
        {u.installing && <Progress value={u.progress} label="Downloading the update" />}
        {(u.error || u.saved || offer?.notes) && (
          <div className="flex flex-col gap-3 border-t border-line p-4">
            {u.error && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{u.error}</Banner>}
            {u.saved && <Banner tone="good" icon={<Download className="h-4 w-4" />}>Saved to {u.saved} and opened in your installer.</Banner>}
            {offer?.notes && <pre className="max-h-56 overflow-y-auto whitespace-pre-wrap font-sans text-xs text-ink [overflow-wrap:anywhere]">{plainNotes(offer.notes)}</pre>}
          </div>
        )}
        {info && !info.self_install && <p className="border-t border-line px-4 py-3 text-xs text-dim">Installed from a package: updates download the new package and open it in your software installer. Development builds never offer updates.</p>}
      </Card>

      <Card title="Files"><FileHandler /></Card>

      <Card title="Move to another computer"><Transfer onReview={onReview} /></Card>

      <Card title="About Unpack">
        <Row k="Version">{info?.version ?? "…"}</Row>
        <Row k="Installed as">{info ? (info.self_install ? "AppImage" : "System package or dev build") : "…"}</Row>
        <Row k="Privacy">Stays on this computer; only update checks and the Store use the network</Row>
        <LinkRow icon={<Code2 className="h-[18px] w-[18px]" />} title="Source code" sub="github.com/aiyu-ayaan/Unpack" to="repo" />
        <LinkRow icon={<Bug className="h-[18px] w-[18px]" />} title="Report a problem" sub="Open an issue on GitHub" to="issues" />
      </Card>

      <Card title="Developer">
        <div className="flex flex-col items-center gap-1 px-4 py-6">
          <img src="/developer.jpg" alt="aiyu-ayaan" width={80} height={80} className="h-20 w-20 rounded-full border border-line object-cover" />
          <p className="mt-2 text-base font-medium text-strong">aiyu-ayaan</p>
          <p className="text-dim">Made Unpack for people new to Linux.</p>
        </div>
        <div className="border-t border-line">
          <LinkRow icon={<UserRound className="h-[18px] w-[18px]" />} title="GitHub" sub="github.com/aiyu-ayaan" to="developer" />
          <LinkRow icon={<Globe className="h-[18px] w-[18px]" />} title="Portfolio" sub="aiyu.co.in" to="site" />
        </div>
      </Card>

      <Card title="Built with">
        {CREDITS.map(([n, l]) => <Row key={n} k={n}>{l}</Row>)}
      </Card>
    </div>
  );
}
