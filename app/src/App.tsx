import { useEffect, useState } from "react";
import { History as HistoryIcon, Library as LibraryIcon, Loader2, MonitorCog, Moon, PackagePlus, Shield, Sun, Terminal } from "lucide-react";
import { api, type SystemInfo } from "./api";
import Install from "./views/Install";
import Library from "./views/Library";
import System from "./views/System";
import History from "./views/History";
import { TitleBar } from "./components/TitleBar";

const NAV = [
  { id: "install", label: "Install", file: "install", icon: PackagePlus },
  { id: "library", label: "Library", file: "library", icon: LibraryIcon },
  { id: "system", label: "System", file: "system", icon: MonitorCog },
  { id: "history", label: "History", file: "history.log", icon: HistoryIcon },
] as const;
type View = (typeof NAV)[number]["id"];
/** Work in progress, shown in the status bar. `progress` null = unknown length. */
export type Task = { label: string; progress: number | null } | null;

function useTheme() {
  const dark = () => !window.matchMedia("(prefers-color-scheme: light)").matches;
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    try { const t = localStorage.getItem("theme"); if (t === "dark" || t === "light") return t; } catch { /* storage blocked */ }
    return dark() ? "dark" : "light";
  });
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    try { localStorage.setItem("theme", theme); } catch { /* storage blocked */ }
  }, [theme]);
  return [theme, () => setTheme((t) => (t === "dark" ? "light" : "dark"))] as const;
}

export default function App() {
  const [view, setView] = useState<View>("install");
  // Bumped after each install so Library and History reload.
  const [version, setVersion] = useState(0);
  const [task, setTask] = useState<Task>(null);
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [sysErr, setSysErr] = useState<string | null>(null);
  const [theme, toggleTheme] = useTheme();
  useEffect(() => { api.system().then(setSys).catch((e) => setSysErr(String(e))); }, []);
  const current = NAV.find((n) => n.id === view)!;

  return (
    <div className="flex h-full flex-col">
      <TitleBar title={`${current.label} — Unpack`} />
      <div className="flex min-h-0 flex-1">
        <nav aria-label="Main" className="flex w-16 shrink-0 flex-col items-center gap-1 border-r border-line bg-side py-3">
          <div className="mb-3 grid h-9 w-9 place-items-center rounded-md border border-accent/40 bg-accent/10 font-mono text-lg font-bold text-accent-ink glow" aria-hidden>
            <Terminal className="h-5 w-5" />
          </div>
          {NAV.map(({ id, label, icon: Icon }) => {
            const on = view === id;
            return (
              <button key={id} onClick={() => setView(id)} aria-current={on ? "page" : undefined}
                className={`group relative flex w-full cursor-pointer flex-col items-center gap-1 py-2 transition-colors ${on ? "text-accent-ink" : "text-dim hover:text-ink"}`}>
                <span className={`absolute left-0 top-1/2 h-8 w-0.5 -translate-y-1/2 rounded-r bg-accent transition-transform duration-200 ${on ? "scale-y-100" : "scale-y-0"}`} aria-hidden />
                <Icon className="h-5 w-5 transition-transform group-active:scale-90" aria-hidden />
                <span className="font-mono text-[10px]">{label}</span>
              </button>
            );
          })}
          <button onClick={toggleTheme} className="mt-auto grid h-9 w-9 cursor-pointer place-items-center rounded-md text-dim transition-colors hover:bg-raised hover:text-ink"
            aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`} title="Toggle theme">
            {theme === "dark" ? <Sun className="h-4 w-4" /> : <Moon className="h-4 w-4" />}
          </button>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col">
          <div className="flex h-9 shrink-0 items-end border-b border-line bg-side" role="presentation">
            <div className="flex h-full items-center gap-2 border-r border-t-2 border-line border-t-accent bg-bg px-4 font-mono text-xs">
              <current.icon className="h-3.5 w-3.5 text-accent-ink" aria-hidden />
              {current.file}
            </div>
          </div>
          <main className="min-h-0 flex-1 overflow-auto bg-bg px-8 py-7">
            <div hidden={view !== "install"}><Install onInstalled={() => setVersion((v) => v + 1)} onTask={setTask} /></div>
            <div key={view} className="enter">
              {view === "library" && <Library version={version} onTask={setTask} />}
              {view === "system" && <System s={sys} err={sysErr} />}
              {view === "history" && <History version={version} />}
            </div>
          </main>
        </div>
      </div>

      <footer className="flex h-6 shrink-0 items-center gap-4 bg-accent px-3 font-mono text-[11px] text-on-accent">
        <span className="flex items-center gap-1.5 font-bold"><Terminal className="h-3 w-3" aria-hidden /> unpack</span>
        {sys ? <>
          <span className="truncate">{sys.distro}</span>
          <span className="hidden sm:inline">{sys.native ?? "no native manager"}</span>
          <span className="hidden sm:inline">{sys.arch}</span>
          {sys.is_root && <span className="flex items-center gap-1 font-bold"><Shield className="h-3 w-3" aria-hidden /> root</span>}
        </> : <span>{sysErr ? "system detection failed" : "detecting system…"}</span>}
        <span className="ml-auto flex items-center gap-2" aria-live="polite">
          {task && <>
            <Loader2 className="h-3 w-3 animate-spin" aria-hidden />
            <span className="max-w-60 truncate">{task.label}</span>
            {task.progress !== null && <span>{Math.round(task.progress * 100)}%</span>}
          </>}
          {!task && <span>ready</span>}
        </span>
      </footer>
    </div>
  );
}
