import { Cpu, Lock, ShieldAlert, XCircle } from "lucide-react";
import type { SystemInfo } from "../api";
import { Banner, PageHeader, Progress } from "../components/ui";

const Chips = ({ title, items, ok = true }: { title: string; items: string[]; ok?: boolean }) => (
  <div>
    <div className="label mb-2">{title}</div>
    <div className="stagger flex flex-wrap gap-1.5">{items.length ? items.map((i) => <span key={i} className={`chip ${ok ? "chip-ok" : "chip-warn"}`}>{i}</span>) : <span className="font-mono text-xs text-dim">none</span>}</div>
  </div>
);

export default function System({ s, err }: { s: SystemInfo | null; err: string | null }) {
  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <PageHeader title="system" sub="What Unpack found, so it can pick the right way to install things." />
      {err && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{err}</Banner>}
      {!s && !err && <Progress value={null} label="Detecting system" />}
      {s && <>
        <section className="card flex items-center gap-4 p-5">
          <div className="grid h-12 w-12 place-items-center rounded-md border border-accent/40 bg-accent/10"><Cpu className="h-6 w-6 text-accent-ink" aria-hidden /></div>
          <div className="min-w-0">
            <div className="font-mono text-lg font-semibold">{s.distro}</div>
            <div className="font-mono text-xs text-dim">family={s.family} arch={s.arch} native={s.native ?? "unknown"}</div>
          </div>
          <div className="ml-auto flex flex-wrap justify-end gap-1.5">
            {s.immutable && <span className="chip chip-warn"><Lock className="h-3 w-3" aria-hidden /> read-only base</span>}
            {s.musl && <span className="chip chip-warn"><ShieldAlert className="h-3 w-3" aria-hidden /> musl libc</span>}
            {s.is_root && <span className="chip chip-warn">running as root</span>}
          </div>
        </section>
        {s.is_root && <Banner tone="warn" icon={<ShieldAlert className="h-4 w-4" />}>Unpack is running as root. Apps meant for your user (Flatpak --user, AppImages, ~/.local) will be installed for root instead. Start Unpack as your normal user; it asks for the admin password only when a step needs it.</Banner>}
        <section className="card flex flex-col gap-5 p-5">
          <Chips title="Package managers" items={s.managers} />
          <Chips title="App runtimes (Flatpak, Snap…)" items={s.runtimes} />
          <Chips title="Helper tools found" items={s.helpers} />
          <Chips title="Helper tools missing (some formats need these)" items={s.missing_helpers} ok={false} />
        </section>
      </>}
    </div>
  );
}
