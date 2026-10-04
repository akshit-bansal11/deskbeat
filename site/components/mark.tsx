/**
 * The Deskbeat mark, the same drawing as assets/deskbeat-light.svg and -dark.svg: four
 * sound bars whose heights fall away to draw the letter D. Inline, so it follows the
 * theme toggle rather than the OS: the tile is the page's ink, the bars its ground,
 * and the shortest bar the Ember that holds against that tile.
 */
export function Mark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 108 108" aria-hidden className={className}>
      <rect width="108" height="108" rx="26" fill="var(--ink)" />
      <rect x="21" y="19" width="13" height="70" rx="6.5" fill="var(--ground)" />
      <rect x="39" y="22" width="13" height="64" rx="6.5" fill="var(--ground)" />
      <rect x="57" y="29" width="13" height="50" rx="6.5" fill="var(--ground)" />
      <rect x="75" y="41" width="13" height="26" rx="6.5" fill="var(--mark-beat)" />
    </svg>
  );
}
