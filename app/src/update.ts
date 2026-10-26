import { useCallback, useEffect, useState } from "react";
import { api, errText, type Channel, type Offer, type UpdateInfo } from "./api";

/** The app's own update: what is installed, what a check found, and the download. Checks once at start, quietly. */
export function useUpdate() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [offer, setOffer] = useState<Offer | null>(null);
  const [checked, setChecked] = useState(false);
  const [checking, setChecking] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Where a package update was saved (AppImage updates restart instead). */
  const [saved, setSaved] = useState<string | null>(null);

  const check = useCallback(async () => {
    setChecking(true);
    setError(null);
    try { setOffer(await api.updateCheck()); setChecked(true); }
    catch (e) { setError(errText(e)); }
    finally { setChecking(false); }
  }, []);

  useEffect(() => {
    api.updateInfo().then(setInfo).then(check).catch((e) => setError(errText(e)));
    const un = api.onUpdateProgress(setProgress);
    return () => { void un.then((f) => f()); };
  }, [check]);

  const install = async () => {
    setInstalling(true);
    setError(null);
    try { setSaved(await api.updateInstall()); }
    catch (e) { setError(errText(e)); }
    finally { setInstalling(false); }
  };
  const setChannel = async (channel: Channel) => {
    await api.updateSetChannel(channel);
    setInfo((i) => i && { ...i, channel });
    await check();
  };
  const skip = async () => {
    await api.updateSkip();
    setInfo((i) => i && offer && { ...i, skipped: offer.version });
  };
  return { info, offer, checked, checking, progress, installing, error, saved, check, install, setChannel, skip };
}
export type Update = ReturnType<typeof useUpdate>;

/** Release notes are Markdown; show them as plain text without the markers. */
export const plainNotes = (md: string) => md.replace(/^#{1,6}\s*/gm, "").replace(/\*\*|__|`/g, "").trim();
