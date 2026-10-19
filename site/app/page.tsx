import {
  ArrowRight,
  AudioLines,
  Clock3,
  Disc3,
  type LucideIcon,
  MicVocal,
  Move,
  SlidersHorizontal,
} from "lucide-react";
import type { Metadata } from "next";
import Link from "next/link";
import { DemoFigure } from "@/components/demo-figure";
import { GithubMark } from "@/components/icons/github";
import { Mark } from "@/components/mark";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Badge } from "@/components/ui/badge";
import { BlurFade } from "@/components/ui/blur-fade";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { GridPattern } from "@/components/ui/grid-pattern";
import { LATEST_RELEASE, REPO } from "@/lib/nav";

export const metadata: Metadata = {
  alternates: { canonical: "/" },
};

const FACTS: { term: string; value: string }[] = [
  { term: "Runs on", value: "Windows 11; macOS and Linux preview" },
  { term: "Music widgets follow", value: "The Spotify desktop app" },
  { term: "Built with", value: "Rust and Direct2D" },
  { term: "Install", value: "One exe, no installer" },
];

const WIDGETS: { title: string; Icon: LucideIcon; body: string }[] = [
  {
    title: "Clock",
    Icon: Clock3,
    body: "Day, time and date, each of which can be hidden, restyled or given a format of its own. It works on its own, with or without Spotify.",
  },
  {
    title: "Visualizer",
    Icon: AudioLines,
    body: "A spectrum of whatever Spotify plays, and only that: not your games or calls. Bars, mirrored bars or a wave, and it can span the whole width of the screen.",
  },
  {
    title: "Lyrics",
    Icon: MicVocal,
    body: "Time-synced lyrics, followed line by line, or word by word where the song has timed words and you turn word timing on in the settings. What is being sung either lights up at once or fills as it is sung. Word timing comes from a community-run LyricsPlus server and is never guessed; line timing comes from LRCLIB and needs no opt-in. Cached on disk.",
  },
  {
    title: "Player",
    Icon: Disc3,
    body: "Album art, title, artist, a progress bar you can click to seek, and previous, play/pause and next.",
  },
  {
    title: "Edit layout",
    Icon: Move,
    body: "The clock and the player have no box. Press Ctrl+Alt+E and drag the day, the time, the date, the art, the title, each button, the progress bar and both times anywhere, one at a time. Shift+click several to move them as one.",
  },
  {
    title: "Every detail is a setting",
    Icon: SlidersHorizontal,
    body: "Four one-click looks, then a font, size, weight, colour, opacity, letter spacing and capitals setting for every piece of text. Each can also run across, down or up, with its letters upright or lying sideways. Everything is stored in one TOML file that reloads when you save it.",
  },
];

const STEPS: { title: string; body: string }[] = [
  {
    title: "It draws only what changed",
    body: "Each widget is compiled, sleeps until something it shows has changed, and is not drawn at all while it is covered. The clock wakes once a minute, lyrics at each new line (every frame while a line is sung, with word timing on), the player once a second.",
  },
  {
    title: "Windows says what is playing",
    body: "The track, its position, the album art and the play, pause and skip buttons come from the Windows media session API, filtered to Spotify. There is no Spotify Web API, no client ID and no OAuth, so it works on free accounts and needs no setup.",
  },
  {
    title: "It listens to Spotify alone",
    body: "The visualizer captures Spotify's own output through WASAPI process loopback, so it moves to the music and not to a game or a call. The audio is turned into bar heights and discarded; nothing is recorded or written to disk.",
  },
];

const LIGHT: string[] = [
  "No webview and no runtime. One Rust process drawing with Direct2D.",
  "Event-driven. The visualizer runs only while there is sound.",
  "Nothing hidden is drawn. A widget under a maximized or fullscreen window stops entirely.",
  "Low-power GPU. On a laptop with two GPUs it uses the integrated one.",
];

const LIMITS: string[] = [
  "The full app needs Windows 11. The visualizer, lyrics and player need the Spotify desktop app playing on the same PC: playback sent to another device through Spotify Connect is not seen.",
  "The macOS and Linux build is a preview, not yet tried on real machines beyond CI. It has no settings panel, edit layout, hotkeys, start at login, vertical text or letter spacing, Wayland may ignore window positions, and GNOME cannot keep widgets below windows. The visualizer listens to the default input, which may be the microphone, and only while Spotify plays. Linux may list widgets in the taskbar and Alt-Tab, and macOS in Mission Control.",
  "The exe is not code-signed, so Windows SmartScreen warns the first time you run it.",
  "Word timing is off by default for new installs; an existing config keeps its word_sync value. It depends on community-run LyricsPlus servers that Inlay does not host or check; line timing from LRCLIB needs no opt-in and is the fallback.",
  "Memory use was measured once at about 54 MB; how it was measured was not recorded.",
  "The settings panel, dragging in edit layout, the transport buttons, the word-by-word fill and Win+D are not checked by automated tests. Some are drawn in CI snapshots; none has been checked by hand.",
];

const INSTALL: string[] = [
  "Download Inlay-<version>.exe from the latest release. There is no installer.",
  "Run it. SmartScreen asks first: More info, then Run anyway.",
  "It lives in the notification area. Double-click its icon for settings, or right-click it for a small menu.",
  "The clock shows at once. For the other three, play something in the Spotify desktop app.",
  "On macOS or Linux (preview), download Inlay-<version>-macos.zip or Inlay-<version>-linux-x86_64.tar.gz instead. The full install guide has the first-run steps.",
];

/** A section's heading block: a short accent eyebrow, a 900 title, a dim lede. */
function Heading({
  eyebrow,
  title,
  children,
}: {
  eyebrow: string;
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <BlurFade>
      <p className="text-primary-text text-[0.9375rem] font-bold">{eyebrow}</p>
      <h2 className="mt-2 max-w-[20ch] text-[2rem] leading-[1.08] font-black tracking-[-0.02em] md:text-5xl">
        {title}
      </h2>
      {children ? (
        <p className="text-dim mt-4 max-w-[64ch] text-base leading-relaxed md:text-lg">
          {children}
        </p>
      ) : null}
    </BlurFade>
  );
}

/** An inline "go deeper" link: accent, bold, with an arrow. */
function MoreLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <Link
      href={href}
      className="text-primary-text inline-flex min-h-11 items-center gap-1.5 rounded-lg text-[0.9375rem] font-bold hover:underline hover:underline-offset-4"
    >
      {children}
      <ArrowRight aria-hidden className="size-4" />
    </Link>
  );
}

const SECTION = "mx-auto max-w-[80rem] scroll-mt-20 px-4 py-14 md:px-6 md:py-20";

export default function Page() {
  return (
    <div id="top" className="min-h-dvh">
      <SiteHeader />

      <main id="main" className="scroll-mt-16">
        {/* Hero, on a faint flat grid */}
        <section className="relative overflow-hidden">
          <GridPattern
            width={35}
            height={35}
            className="fill-transparent stroke-line/50 dark:stroke-line/40"
          />
          <div className="relative mx-auto grid max-w-[80rem] gap-14 px-4 pt-12 pb-16 md:px-6 md:pt-20 md:pb-24 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.15fr)] lg:items-center lg:gap-14">
            {/* No entrance on the headline: it is the largest paint on the page, and a
                fade would leave it invisible until the JavaScript arrives. */}
            <div>
              <p className="bg-card shadow-card text-dim inline-flex rounded-full px-4 py-1.5 text-sm font-bold">
                Windows 11 · macOS and Linux preview · four live widgets
              </p>
              <h1 className="mt-6 max-w-[15ch] text-[2.75rem] text-balance leading-[1.02] font-black tracking-[-0.025em] sm:text-6xl lg:text-7xl">
                Live widgets, set into your Windows desktop.
              </h1>
              <p className="text-dim mt-6 max-w-[56ch] text-lg leading-relaxed md:text-xl">
                Inlay draws a clock, a spectrum visualizer, time-synced lyrics and a
                now-playing player on the Windows 11 desktop, under your windows and
                over the desktop icons. Each one redraws only when something on it
                changes, so it idles near zero. A light alternative to Rainmeter.
              </p>
              <p className="text-dim mt-4 max-w-[56ch] text-base leading-relaxed md:text-lg">
                The clock runs on its own. The other three follow whatever Spotify
                plays: track details come from Windows itself, so there is no Spotify
                login and no client ID.
              </p>

              <div className="mt-8 flex flex-wrap items-center gap-3">
                <Button asChild>
                  <a href={LATEST_RELEASE} target="_blank" rel="noreferrer noopener">
                    Download for Windows
                  </a>
                </Button>
                <Button asChild variant="outline">
                  <Link href="/docs">
                    Read the docs
                    <ArrowRight aria-hidden className="size-4" />
                  </Link>
                </Button>
                <Button asChild variant="ghost">
                  <a href={REPO} target="_blank" rel="noreferrer noopener">
                    <GithubMark className="size-4" />
                    Source
                  </a>
                </Button>
              </div>

              <ul
                aria-label="What it does not need"
                className="mt-8 flex flex-wrap gap-2"
              >
                {[
                  "No Spotify login",
                  "No telemetry",
                  "No installer",
                  "MIT licensed",
                ].map((item) => (
                  <li key={item}>
                    <Badge variant="outline">{item}</Badge>
                  </li>
                ))}
              </ul>
            </div>

            <BlurFade delay={0.1}>
              <DemoFigure />
            </BlurFade>
          </div>
        </section>

        {/* The facts, stated rather than claimed */}
        <div className="mx-auto max-w-[80rem] px-4 md:px-6">
          <BlurFade>
            <dl className="bg-line shadow-card grid gap-px overflow-hidden rounded-[var(--radius-panel)] sm:grid-cols-2 lg:grid-cols-4">
              {FACTS.map((fact) => (
                <div key={fact.term} className="bg-card px-5 py-4">
                  <dt className="label">{fact.term}</dt>
                  <dd className="mt-1 text-base font-bold">{fact.value}</dd>
                </div>
              ))}
            </dl>
          </BlurFade>
        </div>

        {/* What you get */}
        <section id="widgets" className={SECTION}>
          <Heading eyebrow="What you get" title="Four widgets, and no skin engine.">
            A clock that stands on its own, and three widgets that follow the music.
            There are no plugins, no scripts and no user-authored skins: every visual
            and behavioural parameter of the four widgets is a setting instead.
          </Heading>

          <ul className="mt-10 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {WIDGETS.map(({ title, Icon, body }, index) => (
              <li key={title}>
                <BlurFade delay={(index % 3) * 0.05} className="h-full">
                  <div className="bg-card shadow-card border-line hover:border-primary-text h-full rounded-[var(--radius-panel)] border transition-colors motion-reduce:transition-none">
                    <div className="p-6">
                      <span className="bg-accent-soft text-primary-text grid size-11 place-items-center rounded-xl">
                        <Icon aria-hidden className="size-5" />
                      </span>
                      <h3 className="mt-4 text-lg font-bold">{title}</h3>
                      <p className="text-dim mt-2 text-[0.9375rem] leading-relaxed">
                        {body}
                      </p>
                    </div>
                  </div>
                </BlurFade>
              </li>
            ))}
          </ul>
          <p className="mt-3 px-1">
            <MoreLink href="/docs#settings">Everything the settings cover</MoreLink>
          </p>
        </section>

        {/* How it works */}
        <section id="how" className={SECTION}>
          <Heading
            eyebrow="How it works"
            title="One process that sleeps until something changes."
          >
            A Rainmeter setup re-runs interpreted measures on timers and redraws whether
            or not anything changed. The reason Inlay exists is that cost.
          </Heading>

          <ol className="mt-10 grid gap-4 md:grid-cols-3">
            {STEPS.map((item, index) => (
              <li key={item.title}>
                <BlurFade delay={index * 0.05} className="h-full">
                  <Card className="h-full px-6 py-6">
                    <span className="bg-primary text-primary-foreground grid size-9 place-items-center rounded-full text-[0.9375rem] font-black">
                      {index + 1}
                    </span>
                    <div>
                      <h3 className="text-lg leading-snug font-bold">{item.title}</h3>
                      <p className="text-dim mt-2 text-[0.9375rem] leading-relaxed">
                        {item.body}
                      </p>
                    </div>
                  </Card>
                </BlurFade>
              </li>
            ))}
          </ol>

          <BlurFade>
            <Card className="bg-accent-soft mt-4 px-6 py-6 shadow-none">
              <div>
                <h3 className="text-lg font-bold">How it stays light</h3>
                {/* Ink on the tint: the body of a tinted card is never the accent. */}
                <ul className="mt-2 max-w-[76ch] list-disc space-y-1.5 pl-5 text-[0.9375rem] leading-relaxed">
                  {LIGHT.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
                <p className="mt-2">
                  <MoreLink href="/docs#loop">
                    The loop, and what a widget may ask of it
                  </MoreLink>
                </p>
              </div>
            </Card>
          </BlurFade>
        </section>

        {/* What it will not do */}
        <section id="limits" className={SECTION}>
          <div className="grid gap-10 lg:grid-cols-[minmax(0,24rem)_minmax(0,1fr)] lg:gap-16">
            <Heading eyebrow="The cost" title="What it will not do.">
              What is verified and what is not is written down in the repository.
              Nothing here is hidden because it is inconvenient.
            </Heading>

            <BlurFade delay={0.05}>
              <ul className="bg-card shadow-card divide-line divide-y rounded-[var(--radius-panel)] px-5">
                {LIMITS.map((limit) => (
                  <li key={limit} className="py-4 text-[0.9375rem] leading-relaxed">
                    {limit}
                  </li>
                ))}
              </ul>
              <p className="mt-3 px-1">
                <MoreLink href="/docs#limits">Every known limit, labelled</MoreLink>
              </p>
            </BlurFade>
          </div>
        </section>

        {/* Get it */}
        <section id="get" className={SECTION}>
          <Heading eyebrow="Get it" title="One file, from GitHub Releases.">
            Each release carries the Windows exe named for its version, the macOS and
            Linux preview downloads, a{" "}
            <span className="font-mono text-[0.9em]">SHA256SUMS.txt</span> and a build
            provenance attestation, so a download can be checked against the workflow
            that built it.
          </Heading>

          <BlurFade delay={0.05} className="mt-10">
            <Card className="max-w-[44rem]">
              <CardHeader className="flex items-center gap-4 px-6">
                <Mark className="size-12 shrink-0" />
                <div className="min-w-0">
                  <CardTitle className="text-xl font-black">Inlay</CardTitle>
                  <div className="mt-1.5 flex flex-wrap gap-2">
                    <Badge variant="solid">Windows 11</Badge>
                    <Badge variant="outline">Inlay-x.y.z.exe</Badge>
                    <Badge variant="outline">
                      macOS preview: Inlay-x.y.z-macos.zip
                    </Badge>
                    <Badge variant="outline">
                      Linux preview: Inlay-x.y.z-linux-x86_64.tar.gz
                    </Badge>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="px-6">
                <ol className="divide-line divide-y">
                  {INSTALL.map((step, stepIndex) => (
                    <li key={step} className="flex gap-3 py-3 first:pt-0 last:pb-0">
                      <span className="text-primary-text w-4 shrink-0 text-[0.9375rem] font-black">
                        {stepIndex + 1}
                      </span>
                      <span className="text-dim text-[0.9375rem] leading-relaxed">
                        {step}
                      </span>
                    </li>
                  ))}
                </ol>
              </CardContent>
            </Card>
          </BlurFade>

          <BlurFade>
            <div className="mt-10 flex flex-wrap items-center gap-3">
              <Button asChild>
                <a href={LATEST_RELEASE} target="_blank" rel="noreferrer noopener">
                  Download for Windows
                </a>
              </Button>
              <Button asChild variant="outline">
                <Link href="/docs#install">
                  Full install guide
                  <ArrowRight aria-hidden className="size-4" />
                </Link>
              </Button>
            </div>
            <p className="text-dim mt-4 max-w-[68ch] text-[0.9375rem] leading-relaxed">
              Free and MIT licensed. There is no account, no login and no telemetry: the
              source is in the repository, and the release is built from it by GitHub
              Actions.
            </p>
          </BlurFade>
        </section>
      </main>

      <SiteFooter />
    </div>
  );
}
