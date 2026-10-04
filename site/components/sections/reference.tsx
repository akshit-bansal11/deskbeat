import { ExternalLink, Note, P, Section, Sub } from "@/components/section";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Badge } from "@/components/ui/badge";
import { REPO } from "@/lib/nav";

// Each answer is docs/ARCHITECTURE.md's, CONTRIBUTING.md's or the changelog's own.
const DECISIONS: { id: string; question: string; answer: string }[] = [
  {
    id: "engine",
    question: "Why is it not a skin engine?",
    answer:
      "Cost. A Rainmeter setup re-runs interpreted measures on timers and redraws whether or not anything changed. Here each widget is compiled, sleeps until something it shows has changed, and is not drawn at all while it is covered. So there are no plugins, no scripts and no user-authored skins; every visual and behavioural parameter of the four widgets is a setting instead.",
  },
  {
    id: "gsmtc",
    question: "Why the Windows media session and not the Spotify Web API?",
    answer:
      "The media session needs no Spotify Web API, no client ID and no OAuth, so Deskbeat works on free accounts and needs no setup. Track, position, art and transport all come from it, filtered to Spotify's session.",
  },
  {
    id: "loopback",
    question: "Why capture Spotify's process and not everything the PC plays?",
    answer:
      "So the visualizer moves to Spotify and not to a game or a call. System loopback is a setting for when that is wanted.",
  },
  {
    id: "crates",
    question: "Why two crates?",
    answer:
      "crates/core never names a Windows type. That lets every test in it run on any machine in milliseconds, and keeps the parts that are easy to get subtly wrong, such as timing and config migration, away from the parts that can only be checked by looking.",
  },
  {
    id: "box",
    question: "Why do the clock and the player have no box?",
    answer:
      "Each of their elements has its own position, and the window is wrapped around whatever was drawn, so an element can be dragged anywhere without being clipped. The lyrics and the visualizer keep a frame, because it is what sizes them.",
  },
  {
    id: "servers",
    question: "Why is the list of word-timing servers a setting?",
    answer:
      "The LyricsPlus servers are community-run mirrors of the same service, and they come and go. A list in the config can be edited, or emptied, without a new release.",
  },
  {
    id: "windows-crate",
    question: "Why is the windows crate held on 0.62?",
    answer:
      "Its API moves between minor versions, so a bump is a port and gets its own pull request. Dependabot is told to leave it alone.",
  },
];

type Kind = "accepted" | "unverified";

const KIND_VARIANT: Record<Kind, "solid" | "outline"> = {
  accepted: "outline",
  unverified: "solid",
};

// README.md, "What has and has not been tested", and SECURITY.md, "Known limits".
const LIMITS: { kind: Kind; text: string }[] = [
  {
    kind: "accepted",
    text: "It needs Windows 11 and the Spotify desktop app playing on the same PC. Playback sent to another device through Spotify Connect is not seen.",
  },
  {
    kind: "accepted",
    text: "The exe is not code-signed, so SmartScreen warns on first run. A download can be verified against the release's SHA256SUMS.txt or its attestation.",
  },
  {
    kind: "accepted",
    text: "Word timing depends on community-run LyricsPlus servers. When none answers, lyrics fall back to line timing from LRCLIB.",
  },
  {
    kind: "accepted",
    text: "Memory use measured about 54 MB, above the 40 MB this was aiming for.",
  },
  {
    kind: "unverified",
    text: "The settings panel, the colour picker, and dragging and multi-selecting elements in edit layout.",
  },
  {
    kind: "unverified",
    text: "The word-by-word fill staying in time with a song as it plays.",
  },
  { kind: "unverified", text: "The transport buttons and seeking." },
  { kind: "unverified", text: "Staying visible through Show Desktop (Win+D)." },
  { kind: "unverified", text: "Settings carrying over from the Sonic Veil name." },
];

const TESTED: string[] = [
  "All four widgets draw, sit on the desktop layer, and cost no CPU while idle.",
  "Lyrics lookup, title cleaning and the disk cache, against LRCLIB, and word timing from a LyricsPlus server.",
  "Audio capture, both for one app and for the whole system.",
  "Reading the track, position and album art from a running Spotify.",
  "An older config file loading with its sizes and fonts intact.",
];

// Summaries of CHANGELOG.md, one per section, in its order.
const HISTORY: { version: string; date: string; summary: string }[] = [
  {
    version: "Unreleased",
    date: "in the repository, not yet in a release",
    summary:
      "A new mark: four sound bars, centred like a waveform, that draw the letter D, in the README and the tray icon. In edit layout every element of the clock and the player moves alone, and Shift+click moves several as one; dragging one element no longer makes the others jump or shimmer.",
  },
  {
    version: "1.0.1",
    date: "2026-10-03",
    summary:
      "No change to the app itself. The release file is named for its version, releases are published by the workflow's own token, every action is pinned to a commit, and the repository gained its quality gate script, draft builds, contributing and security documents, Dependabot and a winget manifest.",
  },
  {
    version: "1.0.0",
    date: "2026-10-03",
    summary:
      "Renamed from Sonic Veil to Deskbeat, with settings carried over. Lyrics timed word by word from a LyricsPlus server, with LRCLIB as the fallback. A colour picker on every colour setting. Every element of the clock and the player has its own position and its own font, size, weight, colour, opacity, letter spacing and capitals setting; neither widget has a box. The visualizer can span the whole screen, with up to 512 bars, and be flipped. Fonts can be dropped into a folder instead of installed. The tray menu was removed: a double-click opens the settings panel, which holds everything the menu did.",
  },
  {
    version: "0.1.0",
    date: "2026-10-03",
    summary:
      "First build, a pre-release. Four desktop widgets for Spotify in one native app, track details and controls from the Windows media session, Spotify-only audio capture, lyrics from LRCLIB with a disk cache, a settings panel, four one-click looks, an edit mode, a hand-editable config.toml that reloads when saved, global hotkeys, and the snapshot and probe diagnostics.",
  },
];

const BUILT_ON: { name: string; href: string }[] = [
  { name: "windows-rs", href: "https://github.com/microsoft/windows-rs" },
  { name: "RustFFT", href: "https://github.com/ejmahler/RustFFT" },
  { name: "serde", href: "https://serde.rs" },
  { name: "toml", href: "https://github.com/toml-rs/toml" },
];

export function ReferenceSections() {
  return (
    <>
      <Section
        id="decisions"
        eyebrow="Reference"
        title="Design decisions"
        lede={<>The choices that shape the app, and the reason behind each.</>}
      >
        <Accordion
          type="multiple"
          className="bg-card shadow-card rounded-[var(--radius-panel)] px-5"
        >
          {DECISIONS.map((decision) => (
            <AccordionItem key={decision.id} value={decision.id}>
              <AccordionTrigger>{decision.question}</AccordionTrigger>
              <AccordionContent>{decision.answer}</AccordionContent>
            </AccordionItem>
          ))}
        </Accordion>
      </Section>

      <Section
        id="limits"
        eyebrow="Reference"
        title="Known limits"
        lede={
          <>
            What Deskbeat does not do, and what has been checked only by automated tests
            and rendered snapshots, not by hand.
          </>
        }
      >
        <ul className="bg-card shadow-card divide-line divide-y rounded-[var(--radius-card)] px-5">
          {LIMITS.map((limit) => (
            <li
              key={limit.text}
              className="flex flex-col gap-2 py-4 sm:flex-row sm:gap-4"
            >
              <span className="sm:w-28 sm:shrink-0">
                <Badge variant={KIND_VARIANT[limit.kind]}>{limit.kind}</Badge>
              </span>
              <span className="text-[0.9375rem] leading-relaxed">{limit.text}</span>
            </li>
          ))}
        </ul>
        <Note label="What the two labels mean">
          <strong className="text-foreground">accepted</strong> is a cost of how
          Deskbeat is built today.{" "}
          <strong className="text-foreground">unverified</strong> is covered by
          automated tests and rendered snapshots but has not been checked by hand on a
          real desktop.
        </Note>

        <Sub>Tested on a real Windows 11 desktop</Sub>
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          {TESTED.map((item) => (
            <li key={item}>{item}</li>
          ))}
        </ul>
      </Section>

      <Section
        id="history"
        eyebrow="Reference"
        title="Version history"
        lede={
          <>
            The full record is{" "}
            <ExternalLink href={`${REPO}/blob/main/CHANGELOG.md`}>
              CHANGELOG.md
            </ExternalLink>
            , which follows Keep a Changelog. Versions follow Semantic Versioning.
          </>
        }
      >
        <div className="grid gap-3">
          {HISTORY.map((entry) => (
            <div
              key={entry.version}
              className="bg-card shadow-card rounded-[var(--radius-card)] p-5"
            >
              <div className="flex flex-wrap items-baseline gap-3">
                <p className="text-lg font-bold">{entry.version}</p>
                <span className="label">{entry.date}</span>
              </div>
              <p className="text-dim mt-2 max-w-[70ch] text-[0.9375rem] leading-relaxed">
                {entry.summary}
              </p>
            </div>
          ))}
        </div>
      </Section>

      <Section id="credits" eyebrow="Reference" title="Licence and credits" lede="MIT.">
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            Line-timed lyrics come from{" "}
            <ExternalLink href="https://lrclib.net">LRCLIB</ExternalLink>. Word-timed
            lyrics come from community-run LyricsPlus servers.
          </li>
          <li>
            Built on{" "}
            {BUILT_ON.map((item, index) => (
              <span key={item.name}>
                {index > 0 ? (index === BUILT_ON.length - 1 ? " and " : ", ") : null}
                <ExternalLink href={item.href}>{item.name}</ExternalLink>
              </span>
            ))}
            , all MIT or Apache 2.0.
          </li>
          <li>
            The clock&apos;s default pairing, Anurati and Quicksand, follows the Mond
            Rainmeter skin. Neither font is distributed with Deskbeat.
          </li>
          <li>
            Spotify is a trademark of Spotify AB. Deskbeat is not affiliated with or
            endorsed by Spotify; it reads what Windows reports about the app that is
            playing.
          </li>
        </ul>
        <P>
          Deskbeat is free and stays free. The licence is{" "}
          <ExternalLink href={`${REPO}/blob/main/LICENSE`}>MIT</ExternalLink>.
        </P>
      </Section>
    </>
  );
}
