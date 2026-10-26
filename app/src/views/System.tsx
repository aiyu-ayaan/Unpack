import { useEffect, useState } from "react";
import { Cpu, Lock, ShieldAlert } from "lucide-react";
import { api, errText, type SystemInfo } from "../api";

const Chips = ({ title, items, ok = true }: { title: string; items: string[]; ok?: boolean }) => (
  <div>
    <div className="mb-2 text-xs font-semibold uppercase tracking-wider text-dim">{title}</div>
    <div className="flex flex-wrap gap-1.5">{items.length ? items.map((i) => <span key={i} className={`chip ${ok ? "chip-ok" : "chip-warn"}`}>{i}</span>) : <span className="text-sm text-dim">none</span>}</div>
  </div>
);

export default function System() {
  const [s, setS] = useState<SystemInfo | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => { api.system().then(setS).catch((e) => setErr(errText(e))); }, []);
  if (err) return <div className="text-rose-300">{err}</div>;
  if (!s) return null;
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <header><h1 className="text-3xl font-bold tracking-tight">This computer</h1><p className="mt-1 text-dim">What Unpack found, so it can pick the right way to install things.</p></header>
      <section className="glass flex items-center gap-4 p-6">
        <div className="grid h-14 w-14 place-items-center rounded-2xl bg-gradient-to-br from-brand to-brand2"><Cpu className="h-7 w-7 text-white" /></div>
        <div><div className="text-xl font-semibold">{s.distro}</div><div className="text-sm text-dim">family {s.family} · {s.arch} · native manager {s.native ?? "unknown"}</div></div>
        <div className="ml-auto flex flex-wrap justify-end gap-1.5">
          {s.immutable && <span className="chip chip-warn"><Lock className="h-3 w-3" /> read-only base</span>}
          {s.musl && <span className="chip chip-warn"><ShieldAlert className="h-3 w-3" /> musl libc</span>}
          {s.is_root && <span className="chip chip-warn">running as root</span>}
        </div>
      </section>
      <section className="glass flex flex-col gap-5 p-6">
        <Chips title="Package managers" items={s.managers} />
        <Chips title="App runtimes (Flatpak, Snap…)" items={s.runtimes} />
        <Chips title="Helper tools found" items={s.helpers} />
        <Chips title="Helper tools missing (some formats need these)" items={s.missing_helpers} ok={false} />
      </section>
    </div>
  );
}
