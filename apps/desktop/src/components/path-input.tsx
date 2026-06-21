import { useEffect, useState } from "react";
import { FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { browsePath, desktopApi, isNativeRuntime, type PathInfo } from "../bridge";

type PathInputProps = {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  placeholder?: string;
  required?: boolean;
  /** Browse a folder (true) or a file (false). */
  directory?: boolean;
  /** Semicolon-separated list: Browse appends and validation is skipped. */
  multi?: boolean;
  disabled?: boolean;
  className?: string;
};

export function PathInput({ value, onChange, ariaLabel, placeholder, required, directory = true, multi = false, disabled = false, className }: PathInputProps) {
  const [info, setInfo] = useState<PathInfo | null>(null);

  useEffect(() => {
    if (multi || !value.trim() || !isNativeRuntime()) {
      setInfo(null);
      return;
    }
    let active = true;
    const timer = setTimeout(() => {
      desktopApi.pathInfo(value.trim()).then((result) => { if (active) setInfo(result); }).catch(() => undefined);
    }, 300);
    return () => { active = false; clearTimeout(timer); };
  }, [value, multi]);

  const notFound = Boolean(value.trim()) && info !== null && !info.exists;
  const wrongKind = info?.exists && (directory ? !info.isDir : !info.isFile);
  const invalid = notFound || wrongKind;

  async function browse() {
    const selected = await browsePath({ directory, title: ariaLabel });
    if (!selected) return;
    onChange(multi && value.trim() ? `${value.replace(/[;\s]+$/, "")}; ${selected}` : selected);
  }

  return (
    <div className={cn("flex min-w-48 flex-1 flex-col gap-1", className)}>
      <div className="flex gap-2">
        <Input
          aria-label={ariaLabel}
          required={required}
          disabled={disabled}
          placeholder={placeholder}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          aria-invalid={invalid || undefined}
          className={cn("flex-1", invalid && "border-destructive focus-visible:ring-destructive/40")}
        />
        {isNativeRuntime() ? (
          <Button type="button" variant="outline" disabled={disabled} onClick={() => void browse()} aria-label={`Browse for ${ariaLabel}`}>
            <FolderOpen size={15} /> Browse
          </Button>
        ) : null}
      </div>
      {notFound ? <span className="text-xs text-destructive">Path not found.</span>
        : wrongKind ? <span className="text-xs text-destructive">{directory ? "Expected a folder." : "Expected a file."}</span>
        : null}
    </div>
  );
}
