import { useEffect, useState } from "react";
import { RefreshCcw, Store as StoreIcon, History as HistoryIcon, Library as LibraryIcon, Loader2, MonitorCog, Moon, PackagePlus, Settings as SettingsIcon, ShieldAlert, Sun } from "lucide-react";
import { api, type Plan, type SystemInfo, type Upgrade } from "./api";
import Install from "./views/Install";
import Library from "./views/Library";
import Store from "./views/Store";
import Updates from "./views/Updates";
import System from "./views/System";
import History from "./views/History";
import Settings from "./views/Settings";
import { UpdatePrompt } from "./components/UpdatePrompt";
import { useUpdate } from "./update";
import { TitleBar } from "./components/TitleBar";
import { Progress } from "./components/ui";

const NAV = [
  { id: "install", label: "Install", icon: PackagePlus },
  { id: "store", label: "Store", icon: StoreIcon },
  { id: "updates", label: "Updates", icon: RefreshCcw },
  { id: "library", label: "Library", icon: LibraryIcon },
  { id: "system", label: "This computer", icon: MonitorCog },
  { id: "history", label: "History", icon: HistoryIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
] as const;
type View = (typeof NAV)[number]["id"];
/** Work in progress, shown in the status bar. `progress` null = unknown length. */
export type Task = { label: string; progress: number | null } | null;
/** Something for the Install view to show: a plan built elsewhere (Store, Updates, Fix) or an input to check. */
export type Incoming = { plan: Plan } | { input: string };

function useTheme() {
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    try { const t = localStorage.getItem("theme"); if (t === "dark" || t === "light") return t; } catch { /* storage blocked */ }
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
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
  const [incoming, setIncoming] = useState<Incoming | null>(null);
  const review = (i: Incoming) => { setIncoming(i); setView("install"); };
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [sysErr, setSysErr] = useState<string | null>(null);
  const [theme, toggleTheme] = useTheme();
  const update = useUpdate();
  const [upgrades, setUpgrades] = useState<Upgrade[] | null>(null);
  const [upErr, setUpErr] = useState<string | null>(null);
  const loadUpgrades = () => { setUpgrades(null); setUpErr(null); api.upgrades().then(setUpgrades).catch((e) => { setUpErr(String(e)); setUpgrades([]); }); };
  useEffect(() => { api.system().then(setSys).catch((e) => setSysErr(String(e))); }, []);
  // Checked once at start and after each install, so the badge stays current.
  useEffect(loadUpgrades, [version]);
  const current = NAV.find((n) => n.id === view)!;

  return (
    <div className="flex h-full flex-col">
      <TitleBar title={`${current.label} — Unpack`} />
      <div className="flex min-h-0 flex-1">
        <nav aria-label="Main" className="flex w-12 shrink-0 flex-col items-center border-r border-line bg-side">
          {NAV.map(({ id, label, icon: Icon }) => {
            const on = view === id;
            const dot = id === "settings" && !!update.offer;
            const count = id === "updates" ? upgrades?.length ?? 0 : 0;
            return (
              <button key={id} onClick={() => setView(id)} aria-current={on ? "page" : undefined} aria-label={dot ? `${label} (update available)` : count ? `${label} (${count})` : label} title={label}
                className={`relative grid h-12 w-12 cursor-pointer place-items-center transition-colors ${id === "settings" ? "mt-auto" : ""} ${on ? "text-strong" : "text-dim hover:text-strong"}`}>
                <span className={`absolute inset-y-0 left-0 w-0.5 bg-accent-ink transition-opacity duration-150 ${on ? "opacity-100" : "opacity-0"}`} aria-hidden />
                <Icon className="h-6 w-6" strokeWidth={1.5} aria-hidden />
                {dot && <span className="absolute right-2.5 top-2.5 h-2 w-2 rounded-full bg-accent-ink" aria-hidden />}
                {count > 0 && <span className="absolute right-1 top-1.5 min-w-4 rounded-full bg-accent px-1 text-[10px] leading-4 text-on-accent" aria-hidden>{count > 99 ? "99+" : count}</span>}
              </button>
            );
          })}
          <button onClick={toggleTheme} className="grid h-12 w-12 cursor-pointer place-items-center text-dim transition-colors hover:text-strong"
            aria-label={`Switch to ${theme === "dark" ? "light" : "dark"} theme`} title={`Switch to ${theme === "dark" ? "light" : "dark"} theme`}>
            {theme === "dark" ? <Sun className="h-5 w-5" strokeWidth={1.5} /> : <Moon className="h-5 w-5" strokeWidth={1.5} />}
          </button>
        </nav>

        <div className="flex min-w-0 flex-1 flex-col">
          {task && <Progress value={task.progress} label={task.label} />}
          <main className="min-h-0 flex-1 overflow-auto bg-bg px-10 py-8">
            <div hidden={view !== "install"}><Install onInstalled={() => setVersion((v) => v + 1)} onTask={setTask} incoming={incoming} onTaken={() => setIncoming(null)} /></div>
            <div key={view} className="enter">
              {view === "store" && <Store version={version} onReview={review} />}
              {view === "updates" && <Updates list={upgrades} err={upErr} reload={loadUpgrades} onReview={review} />}
              {view === "library" && <Library version={version} onTask={setTask} />}
              {view === "system" && <System s={sys} err={sysErr} />}
              {view === "history" && <History version={version} />}
              {view === "settings" && <Settings u={update} />}
            </div>
          </main>
        </div>
      </div>

      <UpdatePrompt u={update} />

      <footer className="flex h-6 shrink-0 items-center gap-1 border-t border-line bg-side px-2 text-xs text-dim">
        {sys ? <>
          <StatusItem>{sys.distro}</StatusItem>
          {sys.native && <StatusItem>{sys.native}</StatusItem>}
          {sys.is_root && <StatusItem><ShieldAlert className="h-3.5 w-3.5 text-warn" aria-hidden /> Running as administrator</StatusItem>}
        </> : <StatusItem>{sysErr ? "Could not detect this computer" : "Detecting this computer…"}</StatusItem>}
        <span className="ml-auto flex items-center" aria-live="polite">
          {task
            ? <StatusItem><Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden /> <span className="max-w-72 truncate">{task.label}</span>{task.progress !== null && ` · ${Math.round(task.progress * 100)}%`}</StatusItem>
            : <StatusItem>Ready</StatusItem>}
        </span>
      </footer>
    </div>
  );
}

const StatusItem = ({ children }: { children: React.ReactNode }) => <span className="flex h-full items-center gap-1 rounded-sm px-1.5">{children}</span>;

