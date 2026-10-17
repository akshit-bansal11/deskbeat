/**
 * The Inlay mark, the same drawing as assets/inlay.svg: four flat pieces set into one
 * tile, one for each widget. Its colours are the brand's and do not follow the theme.
 */
export function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 108 108" aria-hidden className={className}>
      <rect width="108" height="108" rx="26" fill="#15151C" />
      <path d="M20 20A32 32 0 0 1 52 52H20Z" fill="#3355FF" />
      <circle cx="72" cy="36" r="16" fill="#FF6B2C" />
      <circle cx="36" cy="72" r="16" fill="#FFC93C" />
      <path d="M56 88A32 32 0 0 1 88 56V88Z" fill="#F3EFE6" />
    </svg>
  );
}
