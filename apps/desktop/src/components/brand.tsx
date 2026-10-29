// Pipeline OS brand lockup: the segmented-pipe "P" glyph (matching the app icon)
// plus the wordmark. Used in the sidebar and onboarding.

export function BrandGlyph({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 1024 1024" className={className} role="img" aria-label="Pipeline OS" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <linearGradient id="pos-tile" x1="180" y1="120" x2="900" y2="950" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#1F1D1B" />
          <stop offset="1" stopColor="#0E0D0C" />
        </linearGradient>
        <linearGradient id="pos-pipe" x1="330" y1="300" x2="640" y2="800" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#FDBA74" />
          <stop offset="0.5" stopColor="#FB923C" />
          <stop offset="1" stopColor="#F97316" />
        </linearGradient>
        <linearGradient id="pos-pipe2" x1="470" y1="320" x2="720" y2="660" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#FB923C" />
          <stop offset="0.55" stopColor="#F97316" />
          <stop offset="1" stopColor="#EA580C" />
        </linearGradient>
      </defs>
      <rect x="64" y="64" width="896" height="896" rx="224" fill="url(#pos-tile)" />
      <rect x="80.5" y="80.5" width="863" height="863" rx="208" fill="none" stroke="#FFFFFF" strokeOpacity="0.06" strokeWidth="3" />
      <rect x="340" y="296" width="112" height="484" rx="56" fill="url(#pos-pipe)" />
      <path d="M476 350 H558 A154 154 0 0 1 558 628 H476" fill="none" stroke="url(#pos-pipe2)" strokeWidth="112" strokeLinecap="round" strokeLinejoin="round" />
      <rect x="336" y="470" width="120" height="16" rx="8" fill="#0E0D0C" opacity="0.9" />
      <rect x="336" y="624" width="120" height="16" rx="8" fill="#0E0D0C" opacity="0.9" />
    </svg>
  );
}

export function BrandLockup({ className }: { className?: string }) {
  return (
    <div className={className}>
      <div className="flex items-center gap-2.5">
        <BrandGlyph className="h-9 w-9 shrink-0 rounded-[9px]" />
        <div className="leading-tight">
          <div className="text-[17px] font-semibold tracking-tight">
            <span className="text-foreground">Pipeline</span>
            <span className="text-primary">&nbsp;OS</span>
          </div>
          <span className="text-[10px] uppercase tracking-[0.16em] text-muted-foreground">Creative launcher</span>
        </div>
      </div>
    </div>
  );
}
