/**
 * The Deskbeat mark, the same drawing as assets/deskbeat-light.svg and -dark.svg: a sun
 * coming up off a desk, its rays a spectrum caught mid-song. Inline, so it follows the
 * theme toggle rather than the OS: the tile is the page's ink, the desk and rays its
 * ground, and the sun the Ember that holds against that tile.
 */
export function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 108 108" aria-hidden className={className}>
      <rect width="108" height="108" rx="26" fill="var(--ink)" />
      <g fill="none" stroke="var(--ground)" strokeLinecap="round">
        <path d="M24 83H84" strokeWidth="12" />
        <path
          d="M32.13 64.89 21.66 61.49 M40.48 53.39 28.73 37.21 M54 49 54 22 M67.52 53.39 80.45 35.59 M75.87 64.89 89.19 60.57"
          strokeWidth="10"
        />
      </g>
      <path d="M39 72a15 15 0 0 1 30 0z" fill="var(--mark-beat)" />
    </svg>
  );
}
