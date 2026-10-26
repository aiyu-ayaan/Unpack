import type { ReactNode } from "react";

/** `value` 0..1 shows a determinate bar; `null` shows a sliding indeterminate one. */
export function Progress({ value, label }: { value: number | null; label?: string }) {
  const pct = value === null ? undefined : Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <div role="progressbar" aria-label={label ?? "Progress"} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}
      className="relative h-1 w-full overflow-hidden rounded-full bg-line">
      {pct === undefined
        ? <div className="bar-indet absolute inset-y-0 w-2/5 rounded-full bg-accent" />
        : <div className="h-full rounded-full bg-accent shadow-[0_0_8px_var(--accent)] transition-[width] duration-500 ease-out" style={{ width: `${pct}%` }} />}
    </div>
  );
}

const TONE = { good: "var(--accent-ink)", warn: "var(--warn)", bad: "var(--bad)", info: "var(--info)" };

export function Banner({ tone, icon, children }: { tone: keyof typeof TONE; icon: ReactNode; children: ReactNode }) {
  const c = TONE[tone];
  return (
    <div role={tone === "bad" ? "alert" : undefined} className="enter flex items-start gap-2 rounded-md border p-3 text-sm"
      style={{ color: c, borderColor: `color-mix(in srgb, ${c} 40%, transparent)`, background: `color-mix(in srgb, ${c} 8%, transparent)` }}>
      <span className="mt-0.5 shrink-0" aria-hidden>{icon}</span>
      <div className="min-w-0 whitespace-pre-wrap [overflow-wrap:anywhere]">{children}</div>
    </div>
  );
}

export function PageHeader({ title, sub, children }: { title: string; sub: string; children?: ReactNode }) {
  return (
    <header className="flex items-end justify-between gap-4">
      <div>
        <h1 className="font-mono text-2xl font-bold tracking-tight"><span className="text-accent-ink glow">~/</span>{title}</h1>
        <p className="mt-1 text-dim">{sub}</p>
      </div>
      {children}
    </header>
  );
}

export const Empty = ({ children }: { children: ReactNode }) => <div className="px-5 py-10 text-center font-mono text-sm text-dim">{children}</div>;
