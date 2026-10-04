import { ArrowLeftRight } from "lucide-react";

type Column = {
  title: string;
  subtitle: string;
  rows: { name: string; detail: string }[];
};

const CORE: Column = {
  title: "crates/core",
  subtitle: "Pure logic, no Windows types, unit tested anywhere",
  rows: [
    { name: "lrc", detail: "LRC and enhanced LRC parsing" },
    {
      name: "timing",
      detail: "Line ends, word timing, the invariants the renderer relies on",
    },
    { name: "clock", detail: "The sync clock: where the track is right now" },
    {
      name: "lrclib · lyricsplus",
      detail:
        "LRCLIB requests, title cleaning and choosing among results; word-timed lyrics, request and response parsing",
    },
    { name: "bands", detail: "FFT bins to bars, smoothing" },
    {
      name: "color · timefmt",
      detail: "Hex, HSV, the accent taken from album art; clock formats",
    },
    { name: "config", detail: "The TOML schema, defaults, clamps, migrations" },
  ],
};

const APP: Column = {
  title: "crates/app",
  subtitle: "The exe: everything that needs a window, a device or Spotify",
  rows: [
    {
      name: "UI thread",
      detail: "Message loop, windows, Direct2D drawing, settings panel",
    },
    { name: "media thread", detail: "Windows media session events for Spotify" },
    { name: "capture thread", detail: "WASAPI loopback, FFT, bands" },
    { name: "lyrics thread", detail: "One short-lived thread per track" },
  ],
};

function Col({ column }: { column: Column }) {
  return (
    <div className="bg-card shadow-card overflow-hidden rounded-[var(--radius-panel)]">
      <div className="border-line border-b px-5 py-4">
        <p className="font-mono text-base font-bold">{column.title}</p>
        <p className="text-dim mt-0.5 text-sm">{column.subtitle}</p>
      </div>
      <ul className="divide-line divide-y px-5">
        {column.rows.map((row) => (
          <li key={row.name} className="py-3">
            <p className="font-mono text-[0.8125rem] leading-snug [overflow-wrap:anywhere]">
              {row.name}
            </p>
            <p className="text-dim mt-1 text-sm leading-relaxed">{row.detail}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The map of the two crates, as docs/ARCHITECTURE.md draws it. */
export function FlowDiagram() {
  return (
    <figure>
      <div className="grid gap-4 lg:grid-cols-[1fr_auto_1fr] lg:items-start">
        <Col column={CORE} />
        <div className="flex items-center justify-center gap-2 lg:mt-24 lg:flex-col">
          <span className="bg-accent-soft text-primary grid size-10 place-items-center rounded-full">
            <ArrowLeftRight aria-hidden className="size-4 rotate-90 lg:rotate-0" />
          </span>
        </div>
        <Col column={APP} />
      </div>
      <figcaption className="text-dim mt-4 max-w-[72ch] text-sm leading-relaxed">
        The app calls into the core for every decision that can be tested without a
        screen. Threads other than the UI thread never touch a window: they write to
        shared state and post a message, and the UI thread reads the state when the
        message arrives.
      </figcaption>
    </figure>
  );
}
