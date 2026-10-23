import { useEffect, useState } from "react";
import { AppWindow } from "lucide-react";
import { desktopApi, isNativeRuntime } from "../bridge";

// Real executable icons are extracted natively; cache them for the session so
// lists don't re-request the same icon on every render.
const cache = new Map<string, Promise<string | null>>();

export function AppIcon({ executable, size = 22 }: { executable?: string; size?: number }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    if (executable && isNativeRuntime()) {
      let request = cache.get(executable);
      if (!request) {
        request = desktopApi.appIcon(executable).then((value) => value ?? null).catch(() => null);
        cache.set(executable, request);
      }
      void request.then((value) => { if (active) setSrc(value); });
    } else {
      setSrc(null);
    }
    return () => { active = false; };
  }, [executable]);
  return src
    ? <img className="shrink-0 rounded object-contain" width={size} height={size} src={src} alt="" />
    : <AppWindow size={size} className="shrink-0 text-muted-foreground" aria-hidden />;
}
