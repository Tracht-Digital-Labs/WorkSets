import { useId } from "react";

interface WorkSetsMarkProps {
  size?: number;
  decorative?: boolean;
}

export function WorkSetsMark({ size = 42, decorative = false }: WorkSetsMarkProps) {
  const maskId = useId().replace(/:/g, "");
  return (
    <svg
      className="worksets-mark"
      width={size}
      height={size}
      viewBox="0 0 64 64"
      role={decorative ? undefined : "img"}
      aria-hidden={decorative || undefined}
      aria-label={decorative ? undefined : "WorkSets"}
      focusable="false"
    >
      <defs>
        <mask id={maskId}>
          <rect width="64" height="64" fill="white" />
          <path d="M22 34.5 28 49l4.2-10.5L36.7 49 43 34.5v21H22z" fill="black" />
        </mask>
      </defs>
      <rect x="7" y="14" width="25" height="41" rx="7" transform="rotate(-9 19.5 34.5)" fill="#B5482E" />
      <circle cx="13.5" cy="20" r="2.1" fill="#FFF8F2" />
      <rect x="33" y="14" width="25" height="41" rx="7" transform="rotate(9 45.5 34.5)" fill="#5F7467" />
      <circle cx="50.5" cy="20" r="2.1" fill="#FFF8F2" />
      <rect x="19" y="7" width="26" height="49" rx="7" fill="#292522" mask={`url(#${maskId})`} />
      <circle cx="25" cy="14" r="2.1" fill="#FFF8F2" />
    </svg>
  );
}

interface WordmarkProps {
  compact?: boolean;
}

export function WorkSetsWordmark({ compact = false }: WordmarkProps) {
  return (
    <div className={`wordmark${compact ? " wordmark--compact" : ""}`}>
      <WorkSetsMark size={compact ? 34 : 44} decorative />
      <span className="wordmark__copy">
        <strong>WorkSets</strong>
        {!compact && <span>by Tracht Digital Labs</span>}
      </span>
    </div>
  );
}
