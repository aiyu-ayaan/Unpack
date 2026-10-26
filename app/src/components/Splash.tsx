import { useEffect, useState } from "react";
import { Logo } from "./Logo";

/** Start-up screen: plays the logo intro over the app, then fades away. Click to skip. */
export function Splash() {
  const [phase, setPhase] = useState<"in" | "out" | "gone">("in");
  useEffect(() => {
    const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const out = setTimeout(() => setPhase("out"), reduced ? 400 : 2800);
    const gone = setTimeout(() => setPhase("gone"), reduced ? 600 : 3200);
    return () => { clearTimeout(out); clearTimeout(gone); };
  }, []);
  if (phase === "gone") return null;
  return (
    <div
      onClick={() => setPhase("out")}
      className={`fixed inset-0 z-50 grid place-items-center bg-bg transition-opacity duration-300 ${phase === "out" ? "pointer-events-none opacity-0" : ""}`}
    >
      <div data-tauri-drag-region className="absolute inset-x-0 top-0 h-9" />
      <div className="flex flex-col items-center gap-5">
        <Logo variant="lockup" animate className="w-[360px] max-w-[70vw]" />
        <span className="sr-only">Unpack</span>
        <p className="splash-tag text-sm text-dim">Install anything on Linux</p>
      </div>
    </div>
  );
}
