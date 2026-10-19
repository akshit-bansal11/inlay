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
      "The media session needs no Spotify Web API, no client ID and no OAuth, so Inlay works on free accounts and needs no setup. Track, position, art and transport all come from it, filtered to Spotify's session.",
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
      "The LyricsPlus servers are community-run mirrors that serve lyrics from other services, and they come and go. A list in the config can be edited, or emptied, without a new release.",
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
    text: "The full app needs Windows 11. The visualizer, lyrics and player need the Spotify desktop app playing on the same PC; playback sent to another device through Spotify Connect is not seen.",
  },
  {
    kind: "unverified",
    text: "The macOS and Linux preview (2.1.0) has not been tried on real machines beyond CI. It has no settings panel, edit layout, hotkeys, start at login, vertical text or letter spacing; Wayland may ignore window positions, and GNOME cannot keep widgets below windows.",
  },
  {
    kind: "accepted",
    text: "The exe is not code-signed, so SmartScreen warns on first run. A download can be verified against the release's SHA256SUMS.txt or its attestation.",
  },
  {
    kind: "accepted",
    text: "Word timing is off by default. It depends on community-run LyricsPlus servers that Inlay does not host or check; with it off, or when none answers, lyrics use line timing from LRCLIB.",
  },
  {
    kind: "accepted",
    text: "Memory use was measured once at about 54 MB; how it was measured was not recorded.",
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
  {
    kind: "unverified",
    text: "Settings carrying over from the Deskbeat and Sonic Veil names.",
  },
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
    version: "2.1.0",
    date: "2026-10-20",
    summary:
      "A preview for macOS and Linux: the same four widgets, reading the same config.toml format, with Spotify read through AppleScript or MPRIS and a tray or menu-bar icon; not yet tried on real machines beyond CI, and without the settings panel, edit layout, hotkeys or start at login. The Windows exe carries version information. Word-by-word timing is off by default, with a switch in settings, and the lyrics widget wakes at each new line instead of redrawing every frame. Lyrics servers must use https. Programs the app starts are no longer named bare. The repository is now akshit-bansal11/inlay and the website inlay.akshitbansal.dev.",
  },
  {
    version: "2.0.0",
    date: "2026-10-17",
    summary:
      "Deskbeat is now Inlay. The exe is inlay.exe, released as Inlay-<version>.exe; settings, fonts, the lyrics cache and the start-with-Windows entry move over on the first run. A new mark of four flat pieces, one for each widget, in a new palette. The website moved to inlay.akshitbansal.dev, and the README, the website and the other documents describe Inlay as a desktop widget app rather than a Spotify add-on.",
  },
  {
    version: "1.4.0",
    date: "2026-10-10",
    summary:
      "Widgets scale with the height of the screen, so a layout made on a big monitor keeps its proportions on a laptop. A dragged widget is measured from the edge, corner or middle of the screen it sits nearest. Widgets left off the edge after the screen changed are kept on the screen again.",
  },
  {
    version: "1.3.0",
    date: "2026-10-04",
    summary:
      "The exe has an icon. The visualizer gained bar width and bar gap in pixels, a quarter-turn rotation, an Across slider and a switch for dots at rest. Word by word lights one word at a time. The position sliders of the lyrics and the visualizer place their middle.",
  },
  {
    version: "1.2.0",
    date: "2026-10-04",
    summary:
      "Lyrics have two modes: what is being sung lights up at once, or fills from its start to its end. Sliders for where each element sits, with a notch at every quarter of the screen. The player can show the name of the song alone. The settings panel is reorganised into a sidebar of five pages. Lyrics are only followed word by word with times a server gave. Word timing no longer goes missing for many songs.",
  },
  {
    version: "1.1.0",
    date: "2026-10-04",
    summary:
      "Any text of the clock and the player can run across, down or up, with its letters upright or lying sideways. A menu on a right-click of the tray icon: the name and version, a link to the repository, the start-with-Windows switch and Quit. A website, with the documentation on one page. A new mark, four sound bars that draw the letter D, in the three colours everything of the app's own now uses: ink, paper and one accent; the tray icon, the tray menu and the settings panel use the same three. In edit layout every element of the clock and the player moves alone, and Shift+click moves several as one; dragging one element no longer makes the others jump or shimmer.",
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
            What Inlay does not do, and what no automated test checks and nobody has
            checked by hand.
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
          <strong className="text-foreground">accepted</strong> is a cost of how Inlay
          is built today. <strong className="text-foreground">unverified</strong> is not
          checked by automated tests, though some of it is drawn in CI snapshots, and
          has not been checked by hand.
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
            Rainmeter skin. Neither font is distributed with Inlay.
          </li>
          <li>
            Spotify is a trademark of Spotify AB. Inlay is not affiliated with or
            endorsed by Spotify; it reads what Windows reports about the app that is
            playing.
          </li>
        </ul>
        <P>
          Inlay is free and stays free. The licence is{" "}
          <ExternalLink href={`${REPO}/blob/main/LICENSE`}>MIT</ExternalLink>.
        </P>
      </Section>
    </>
  );
}
