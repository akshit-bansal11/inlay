import { FlowDiagram } from "@/components/flow-diagram";
import { C, ExternalLink, Note, P, Section, Sub } from "@/components/section";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { REPO } from "@/lib/nav";

const SOURCES: { what: string; from: string; detail: string }[] = [
  {
    what: "Track, position, art and transport",
    from: "The Windows media session API (GSMTC)",
    detail:
      "Filtered to Spotify's session. There is no Spotify Web API, no client ID and no OAuth, so it works on free accounts and needs no setup.",
  },
  {
    what: "Audio",
    from: "WASAPI process loopback",
    detail:
      "On Spotify's process tree, so the visualizer moves to Spotify and not to a game or a call. System loopback is a setting for when that is wanted.",
  },
  {
    what: "Lyrics",
    from: "LRCLIB, then a LyricsPlus server",
    detail:
      "LRCLIB answers first, with the time each line starts. With word timing on (off by default for new installs; an existing config keeps its setting), a LyricsPlus server, when it has the track, then replaces that with the time each syllable is sung.",
  },
];

const WAKES: { answer: string; means: string }[] = [
  { answer: "Idle", means: "Nothing until an event arrives." },
  {
    answer: "After(duration)",
    means:
      "Wake me then. The clock asks for the next minute, the lyrics for each new line, the player for the next second.",
  },
  {
    answer: "Frame",
    means:
      "Something is moving. The loop waits on the compositor's vblank and comes back. With word timing on, the lyrics ask for this every frame while a line is sung.",
  },
];

const MODEL: { title: string; body: React.ReactNode }[] = [
  {
    title: "No account, no login, no telemetry",
    body: "There is no Spotify client ID and no token. Track details and the play, pause and skip buttons go through the Windows media session API, the same one the volume flyout uses.",
  },
  {
    title: "What leaves the PC is the track you are playing, to two lyric services",
    body: (
      <>
        For each new track, its title, artist, album and length are sent over HTTPS to
        LRCLIB and, when word timing is on, to the LyricsPlus servers listed under{" "}
        <C>word_servers</C> in the config. Nothing else is sent: no identifier, no
        listening history, no audio. Turning the lyrics widget off stops both;{" "}
        <C>word_sync = false</C> stops the second.
      </>
    ),
  },
  {
    title: "The LyricsPlus servers are community-run mirrors",
    body: "They are not a service with a contract. They serve lyrics from other services (one answered from Apple Music's catalog when checked); Inlay does not host or check them, and only https servers are accepted. Whoever runs one sees the requests above, and your IP address as any web server does. The list is yours to edit or empty.",
  },
  {
    title: "Audio never leaves the process",
    body: "The visualizer captures Spotify's output through WASAPI loopback, turns it into bar heights and discards it. Nothing is recorded or written to disk.",
  },
  {
    title: "Lyric text is treated as text",
    body: "A response is parsed as JSON and drawn with DirectWrite. Nothing in it is executed, and nothing from it reaches a shell, a path or a URL.",
  },
  {
    title: "It runs as you, with no elevation",
    body: (
      <>
        It writes only its own folders, <C>%APPDATA%\inlay</C> and{" "}
        <C>%LOCALAPPDATA%\inlay</C>, and one value, <C>Inlay</C>, under{" "}
        <C>HKCU\...\Run</C> when Start with Windows is on.
      </>
    ),
  },
  {
    title: "No secrets in the repository",
    body: "The release workflow has none to hold: its only credential is the job's own token.",
  },
];

export function InternalsSections() {
  return (
    <>
      <Section
        id="sources"
        eyebrow="How it works"
        title="Where the data comes from"
        lede={
          <>
            Three sources, all of them on the PC or one HTTPS request away. None of them
            is Spotify&apos;s own API.
          </>
        }
      >
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>What</TableHead>
              <TableHead>From</TableHead>
              <TableHead>Detail</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {SOURCES.map((row) => (
              <TableRow key={row.what}>
                <TableCell className="font-bold">{row.what}</TableCell>
                <TableCell>{row.from}</TableCell>
                <TableCell className="text-dim min-w-[18rem] leading-relaxed">
                  {row.detail}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Section>

      <Section
        id="lyrics"
        eyebrow="How it works"
        title="Lyrics sources"
        lede={
          <>
            Line timing comes from{" "}
            <ExternalLink href="https://lrclib.net">LRCLIB</ExternalLink>. Word timing
            is off by default and comes from community-run LyricsPlus servers.
          </>
        }
      >
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="solid">LRCLIB: when each line starts</Badge>
          <Badge variant="outline">LyricsPlus: when each syllable is sung</Badge>
        </div>
        <P>
          LRCLIB knows when each line starts and nothing more, so word timing built from
          it alone is a guess: a line&apos;s time is spread over its words by length,
          and is marked as a guess. A LyricsPlus server carries the time every syllable
          is sung, and when it has the track that replaces the guess. Each word then
          fills in as it is sung and stays lit afterwards.
        </P>
        <P>
          Word timing from a server is used as it is: words can overlap, as with
          background vocals, and can leave gaps.
        </P>

        <Sub>The cache</Sub>
        <P>
          Both answers are cached on disk, in <C>%LOCALAPPDATA%\inlay</C>. A miss is
          remembered for a week. A server that is down is not remembered as a miss, so
          the track is asked for again next time.
        </P>

        <Sub>The servers</Sub>
        <P>
          The servers asked for word timing are <C>word_servers</C> under{" "}
          <C>[lyrics]</C>, tried in order, and only when word timing is on (the settings
          switch, or <C>word_sync = true</C>); it is off by default for new installs.
          Only https servers are accepted. They are community-run mirrors serving lyrics
          from other services (one answered from Apple Music&apos;s catalog when
          checked); Inlay does not host or check them. They come and go, which is why
          the list is a setting and not only a constant in the code. When none answers,
          or with word timing off, lyrics use line timing from LRCLIB.
        </P>

        <Sub>Keeping lyrics in time</Sub>
        <P>
          The media session reports a position now and then, not continuously, and the
          reports jitter. The sync clock advances on its own between reports, ignores
          jitter inside a deadband, takes the median of recent errors so one outlier
          cannot move it, eases small corrections in over a few frames, and snaps on a
          seek or a resume.
        </P>
        <P>
          If a track&apos;s lyrics run early or late anyway, <C>Ctrl+Alt+Up</C> and{" "}
          <C>Ctrl+Alt+Down</C> move them 100 ms at a time.
        </P>
      </Section>

      <Section
        id="architecture"
        eyebrow="How it works"
        title="Architecture"
        lede={
          <>
            One Windows process, two crates. The reason it exists is cost: each widget
            is compiled, sleeps until something it shows has changed, and is not drawn
            at all while it is covered.
          </>
        }
      >
        <FlowDiagram />

        <Sub>The split</Sub>
        <P>
          <C>crates/core</C> never names a Windows type. That is a rule, not a tendency:
          it is what lets every test in it run on any machine in milliseconds, and it
          keeps the parts that are easy to get subtly wrong, such as timing and config
          migration, away from the parts that can only be checked by looking.
        </P>
        <P>
          <C>crates/app</C> is everything that needs a window, a device or Spotify.
        </P>

        <Sub>Settings and state</Sub>
        <P>
          The settings panel is an ordinary window drawn by the app itself,
          immediate-mode: each frame it lays its rows out from the config and writes
          changes straight back.
        </P>
      </Section>

      <Section
        id="loop"
        eyebrow="How it works"
        title="The loop"
        lede={
          <>
            One thread owns every window. Each pass it drains the message queue, ticks
            each widget, and draws the ones that said they changed.
          </>
        }
      >
        <P>A widget&apos;s tick returns what it wants next:</P>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Answer</TableHead>
              <TableHead>Means</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {WAKES.map((row) => (
              <TableRow key={row.answer}>
                <TableCell className="font-mono text-[0.8125rem] whitespace-nowrap">
                  {row.answer}
                </TableCell>
                <TableCell className="text-dim min-w-[16rem] leading-relaxed">
                  {row.means}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <P>
          The most impatient answer wins. When every widget is idle the thread blocks on
          the message queue and uses no CPU.
        </P>
        <Note label="The rule the project exists for">
          <C>Frame</C> is reserved for motion that is actually on screen: a scroll
          between lyric lines, a word filling in, the visualizer while there is sound.
          Returning it otherwise is the bug this project exists to not have.
        </Note>
      </Section>

      <Section
        id="drawing"
        eyebrow="How it works"
        title="Drawing"
        lede={
          <>
            Each widget is its own layered, click-through, no-activate tool window with
            a DirectComposition swap chain, drawn with Direct2D and DirectWrite through
            one shared device.
          </>
        }
      >
        <P>
          Widgets sit at the bottom of the window stack, above the desktop and under
          every app. A widget under a maximized or fullscreen window is not drawn. On a
          laptop with two GPUs it uses the integrated one.
        </P>
        <P>
          The clock and the player have no box. Each of their elements has its own
          position, and the window is wrapped around whatever was drawn. In edit mode
          that window covers the work area instead and does not move, so dragging one
          element cannot disturb the others; the space between elements passes clicks
          through. The lyrics and the visualizer keep a frame, because it is what sizes
          them.
        </P>
      </Section>

      <Section
        id="security"
        eyebrow="How it works"
        title="Privacy and security"
        lede={
          <>
            Inlay draws on your desktop, listens to what Spotify plays and asks the
            internet for lyrics. This is what it touches and what leaves the PC.
          </>
        }
      >
        <div className="grid gap-3">
          {MODEL.map((item) => (
            <div
              key={item.title}
              className="bg-card shadow-card rounded-[var(--radius-card)] p-5"
            >
              <p className="text-base font-bold">{item.title}</p>
              <p className="text-dim mt-2 max-w-[72ch] text-[0.9375rem] leading-relaxed">
                {item.body}
              </p>
            </div>
          ))}
        </div>

        <Sub>Known limits</Sub>
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            The exe is not code-signed, so SmartScreen warns on first run. Verify a
            download against the release&apos;s <C>SHA256SUMS.txt</C>, or with{" "}
            <C>gh attestation verify</C>.
          </li>
          <li>
            Anyone who can write <C>%APPDATA%\inlay\config.toml</C> can point{" "}
            <C>word_servers</C> at a server of their choosing and learn what you play.
            That person can already run programs as you.
          </li>
          <li>
            Fonts in <C>%APPDATA%\inlay\fonts</C> are loaded by DirectWrite. A malicious
            font file is a risk for any program that renders it; only put fonts there
            that you trust.
          </li>
        </ul>

        <Sub>Reporting a hole</Sub>
        <P>
          Open a{" "}
          <ExternalLink href={`${REPO}/security/advisories/new`}>
            security advisory
          </ExternalLink>{" "}
          on GitHub; it reaches the maintainer privately. Please do not open a public
          issue for something exploitable. Expect an acknowledgement within a week.
        </P>

        <Sub>This site</Sub>
        <P>
          The site is plain static files. It sets no cookie, runs no analytics and loads
          nothing from another origin: the fonts are served from here, and its
          Content-Security-Policy allows only this origin.
        </P>
      </Section>
    </>
  );
}
