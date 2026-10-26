import { useState } from "react";
import { History as HistoryIcon, Library as LibraryIcon, MonitorCog, PackagePlus } from "lucide-react";
import Install from "./views/Install";
import Library from "./views/Library";
import System from "./views/System";
import History from "./views/History";

const NAV = [
  { id: "install", label: "Install", icon: PackagePlus },
  { id: "library", label: "Library", icon: LibraryIcon },
  { id: "system", label: "This computer", icon: MonitorCog },
  { id: "history", label: "History", icon: HistoryIcon },
] as const;
type View = (typeof NAV)[number]["id"];

export default function App() {
  const [view, setView] = useState<View>("install");
  // Bumped after each install so Library and History reload.
  const [version, setVersion] = useState(0);
  return (
    <div className="flex h-full">
      <aside className="flex w-60 shrink-0 flex-col gap-1 border-r border-line bg-black/20 p-4">
        <div className="mb-6 flex items-center gap-3 px-2">
          <div className="grid h-9 w-9 place-items-center rounded-xl bg-gradient-to-br from-brand to-brand2 font-bold text-white">U</div>
          <div className="text-lg font-bold tracking-tight">Unpack</div>
        </div>
        {NAV.map(({ id, label, icon: Icon }) => (
          <button key={id} onClick={() => setView(id)}
            className={`flex items-center gap-3 rounded-xl px-3 py-2.5 text-left text-sm font-medium transition ${view === id ? "bg-white/10 text-white" : "text-dim hover:bg-white/5 hover:text-ink"}`}>
            <Icon className="h-4 w-4" /> {label}
          </button>
        ))}
        <div className="mt-auto px-2 text-xs text-dim">Install anything on Linux.</div>
      </aside>
      <main className="flex-1 overflow-auto p-8">
        <div hidden={view !== "install"}><Install onInstalled={() => setVersion((v) => v + 1)} /></div>
        {view === "library" && <Library version={version} />}
        {view === "system" && <System />}
        {view === "history" && <History version={version} />}
      </main>
    </div>
  );
}
