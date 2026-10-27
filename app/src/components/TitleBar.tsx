import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { Logo } from "./Logo";

/** Our own window toolbar (native decorations are off). Empty areas drag the window; double-click maximizes. */
export function TitleBar({ title }: { title: string }) {
  const [win] = useState(getCurrentWindow);
  const [max, setMax] = useState(false);
  useEffect(() => {
    void win.isMaximized().then(setMax);
    const un = win.onResized(() => { void win.isMaximized().then(setMax); });
    return () => { void un.then((f) => f()); };
  }, [win]);

  const ctl = "grid h-full w-11 place-items-center text-dim transition-colors hover:bg-hover hover:text-ink";
  return (
    <header data-tauri-drag-region className="flex h-9 shrink-0 select-none items-center border-b border-line bg-side">
      <div className="pointer-events-none flex w-36 items-center gap-2 pl-3">
        <Logo small className="pointer-events-none h-4 w-4" />
        <span className="pointer-events-none text-xs font-medium">{import.meta.env.DEV ? "Unpack (dev)" : "Unpack"}</span>
      </div>
      <div data-tauri-drag-region className="flex-1 truncate text-center text-xs text-dim">{title}</div>
      <div className="flex h-full w-36 justify-end">
        <button className={ctl} onClick={() => void win.minimize()} aria-label="Minimize"><Minus className="h-4 w-4" /></button>
        <button className={ctl} onClick={() => void win.toggleMaximize()} aria-label={max ? "Restore" : "Maximize"}>
          {max ? <Copy className="h-3.5 w-3.5 -scale-x-100" /> : <Square className="h-3.5 w-3.5" />}
        </button>
        <button className={`${ctl} hover:bg-[#e81123]! hover:text-white!`} onClick={() => void win.close()} aria-label="Close"><X className="h-4 w-4" /></button>
      </div>
    </header>
  );
}
