import { CodeBlock } from "@/components/code-block";
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
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { LATEST_RELEASE, REPO } from "@/lib/nav";

const WIDGETS: { title: string; body: string }[] = [
  {
    title: "Clock",
    body: "Day, time and date. It works on its own, with or without Spotify.",
  },
  {
    title: "Visualizer",
    body: "A spectrum of Spotify's audio only, not your games or calls. Bars, mirrored bars or a wave.",
  },
  {
    title: "Lyrics",
    body: "Time-synced lyrics, followed line by line, or word by word where the song has timed words and you turn word timing on in the settings. What is being sung either lights up at once or fills as it is sung. Word timing comes from a community-run LyricsPlus server and is never guessed; line timing comes from LRCLIB and needs no opt-in. Cached on disk.",
  },
  {
    title: "Player",
    body: "Album art, title, artist, a progress bar you can click to seek, and previous, play/pause and next.",
  },
];

// Ctrl+Alt plus the key, from HOTKEYS in crates/app/src/app.rs. The lyric nudge is
// OFFSET_STEP_MS there.
const HOTKEYS: { keys: string; does: string }[] = [
  { keys: "Ctrl+Alt+S", does: "Open settings" },
  {
    keys: "Ctrl+Alt+E",
    does: "Edit layout: drag any element of the clock or player on its own, or drag the lyrics or visualizer to move them",
  },
  { keys: "Ctrl+Alt+H", does: "Hide or show every widget" },
  { keys: "Ctrl+Alt+Up", does: "Show lyrics 100 ms earlier" },
  { keys: "Ctrl+Alt+Down", does: "Show lyrics 100 ms later" },
];

const LAYERS: { name: string; value: string; body: string }[] = [
  {
    name: "On the desktop",
    value: "desktop",
    body: "On the desktop, under every window. The default.",
  },
  {
    name: "Normal window",
    value: "normal",
    body: "An ordinary window: whatever is clicked covers it.",
  },
  { name: "Always on top", value: "top", body: "Above everything." },
];

// The five tabs are TABS in crates/app/src/settings.rs; the rows are that file's.
const PANEL: { tab: string; covers: string }[] = [
  {
    tab: "General",
    covers:
      "Four one-click looks (Glass, Minimal, Neon, Mono). The theme: font, accent, text and card colours, card opacity, corner and border, a shadow under text. The layer the widgets sit on, pausing behind fullscreen apps, hiding when Spotify is closed, global hotkeys. Hide all widgets, Start with Windows, and Quit.",
  },
  {
    tab: "Clock",
    covers:
      "Show it, put a card behind it, put its three rows back in place. A page each for the day, the time and the date: show it, pick what it shows, its position in screen pixels, and its font, size, weight, colour, opacity, letter spacing and capitals, which way it runs (across, down or up) and whether its letters stand upright or lie sideways.",
  },
  {
    tab: "Player",
    covers:
      "Show it, and its background: blurred album art, a card, or none. Two ready-made arrangements, Art on the left and Centred stack. Album art size and corner rounding. The progress bar's length, thickness, colour and unplayed part. The buttons' size and colour. The title, artist, time played and track length, each on a page of its own, positioned and styled like any other text, with the same Runs and Letters rows. Song name only, for the title.",
  },
  {
    tab: "Lyrics",
    covers:
      "Show them, a card behind them, whether what is being sung lights up at once or fills, word by word where available, their position and size, alignment, font, size, weight, lines above and below, line spacing, how much larger the current line is and how lines fade with distance. Colours for the current line, the current word and the other lines, an outline, a shadow, and timing: show earlier or later, and the scroll time.",
  },
  {
    tab: "Visualizer",
    covers:
      "Show it, a card behind it, its style (bars, mirrored bars or a wave), what it listens to (Spotify only, or everything), spanning the whole screen, turning it a quarter so the bars run down the screen, the number of bars, their width, the gap between them, roundness, dots for bars at rest, bass in the middle, and flipping left to right or upside down. Its colour: the accent, one colour or a gradient. Its motion: sensitivity, rise, fall, frames per second, treble boost and pitch range.",
  },
];

// Section and key names from crates/core/src/config.rs. Defaults are that file's.
const CONFIG: { key: string; value: string; means: string }[] = [
  {
    key: "general.layer",
    value: '"desktop"',
    means: 'Where the widgets sit among other windows: "desktop", "normal" or "top".',
  },
  {
    key: "general.pause_on_fullscreen",
    value: "true",
    means: "Stop drawing while a fullscreen app or game is in front.",
  },
  {
    key: "general.hide_without_spotify",
    value: "true",
    means: "Hide the player, lyrics and visualizer while Spotify is not running.",
  },
  { key: "general.hotkeys", value: "true", means: "The global hotkeys." },
  {
    key: "theme.font",
    value: '"Segoe UI Variable Display"',
    means: "The font every text uses unless it names its own.",
  },
  {
    key: "theme.accent",
    value: '"auto"',
    means: '"auto" takes the colour from the album art; otherwise a hex colour.',
  },
  {
    key: "clock.time_format",
    value: '"%H:%M"',
    means: "A hand-written clock format. day_format and date_format work the same way.",
  },
  {
    key: "clock.day.size",
    value: "34.0",
    means:
      "Every element is a table of its own: x, y, align, font, size, weight, color, opacity, spacing, uppercase, direction, letters.",
  },
  {
    key: "clock.day.direction",
    value: '"horizontal"',
    means:
      'Which way a text of the clock or the player runs: "horizontal", "down" (top to bottom) or "up" (bottom to top).',
  },
  {
    key: "clock.day.letters",
    value: '"upright"',
    means:
      '"upright", or "sideways": turned a quarter, lying along the line, whichever way it runs.',
  },
  {
    key: "player.background",
    value: '"art-blur"',
    means: '"art-blur", "card" or "none".',
  },
  {
    key: "lyrics.mode",
    value: '"complete"',
    means:
      '"complete" lights up what is being sung at once; "progress" fills it from its start to its end. With lyrics.word_sync that is each word, where the song has timed words, and the line otherwise.',
  },
  {
    key: "lyrics.offset_ms",
    value: "0",
    means: "Positive shows lyrics earlier. Clamped to ten seconds either way.",
  },
  {
    key: "lyrics.word_sync",
    value: "false",
    means:
      "Ask the servers below for the time each word is sung. The settings switch turns it on; line timing from LRCLIB needs no opt-in.",
  },
  {
    key: "lyrics.word_servers",
    value: "two LyricsPlus mirrors",
    means:
      "The servers asked for word timing, tried in order. Yours to edit or empty; only https servers are accepted.",
  },
  {
    key: "visualizer.style",
    value: '"bars"',
    means: '"bars", "mirror" or "wave".',
  },
  {
    key: "visualizer.source",
    value: '"spotify"',
    means: '"spotify" listens to Spotify alone; "system" to everything the PC plays.',
  },
  {
    key: "visualizer.bars",
    value: "120",
    means: "Between 4 and 512.",
  },
  {
    key: "visualizer.full_width",
    value: "true",
    means: "Span the whole width of the screen, ignoring the frame's width.",
  },
];

const CLOCK_FORMATS: { spec: string; shows: string }[] = [
  { spec: "%H  %k", shows: "Hour, 00-23 or 0-23" },
  { spec: "%I  %l", shows: "Hour, 01-12 or 1-12" },
  { spec: "%M  %S", shows: "Minute, second" },
  { spec: "%p", shows: "AM or PM" },
  { spec: "%A  %a", shows: "Weekday, long or short" },
  { spec: "%B  %b", shows: "Month name, long or short" },
  { spec: "%d  %e", shows: "Day of the month, 01-31 or 1-31" },
  { spec: "%m", shows: "Month, 01-12" },
  { spec: "%Y  %y", shows: "Year, four digits or two" },
  { spec: "%%", shows: "A percent sign" },
];

export function ProductSections() {
  return (
    <>
      <Section
        id="overview"
        eyebrow="Start here"
        title="What Inlay is"
        lede={
          <>
            Live widgets, set into your Windows 11 desktop, in one small native app: a
            clock, a spectrum visualizer, time-synced lyrics and a now-playing player.
          </>
        }
      >
        <P>
          It is built as a light alternative to a Rainmeter setup. The widgets are drawn
          on the desktop itself, under your windows and over the desktop icons, and
          redraw only when something on them changes: the visualizer runs only while
          there is sound, and nothing is drawn at all while a window covers the desktop.
        </P>
        <P>
          It is not a skin engine. There are no plugins, no scripts and no user-authored
          skins; every visual and behavioural parameter of the four widgets is a setting
          instead.
        </P>
        <P>
          The clock works on its own. The visualizer, the lyrics and the player follow
          whatever Spotify plays. There is no Spotify login and no client ID: track
          details and controls come from Windows itself, so it works on free accounts,
          and the visualizer captures Spotify&apos;s audio alone.
        </P>

        <Sub>What you get</Sub>
        <div className="grid gap-3 sm:grid-cols-2">
          {WIDGETS.map((widget) => (
            <div
              key={widget.title}
              className="bg-card shadow-card rounded-[var(--radius-card)] p-5"
            >
              <p className="text-base font-bold">{widget.title}</p>
              <p className="text-dim mt-2 text-[0.9375rem] leading-relaxed">
                {widget.body}
              </p>
            </div>
          ))}
        </div>

        <Note label="Formerly Deskbeat, and before that Sonic Veil">
          Settings, fonts and the lyrics cache kept under either earlier name, in the{" "}
          <C>deskbeat</C> or <C>sonic-veil</C> folder of <C>%APPDATA%</C> and{" "}
          <C>%LOCALAPPDATA%</C>, are moved over the first time <C>inlay.exe</C> runs. An
          old <C>Deskbeat</C> or <C>SonicVeil</C> start-with-Windows entry is replaced
          by one named <C>Inlay</C>.
        </Note>
      </Section>

      <Section
        id="install"
        eyebrow="Start here"
        title="Download and install"
        lede={
          <>
            Download <C>Inlay-&lt;version&gt;.exe</C> from the{" "}
            <ExternalLink href={LATEST_RELEASE}>latest GitHub release</ExternalLink> and
            run it. There is no installer.
          </>
        }
      >
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="solid">Windows 11</Badge>
          <Badge variant="outline">The Spotify desktop app</Badge>
          <Badge variant="outline">Inlay-x.y.z.exe</Badge>
        </div>
        <ol className="text-dim max-w-[68ch] list-decimal space-y-3 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            Download the exe. It is named for its version, so several downloads on one
            PC can be told apart.
          </li>
          <li>
            Run it. The exe is unsigned, so Windows SmartScreen will warn the first
            time: choose <strong className="text-foreground">More info</strong>, then{" "}
            <strong className="text-foreground">Run anyway</strong>.
          </li>
          <li>
            It lives in the notification area. Double-click its icon for settings, or
            right-click it for a small menu.
          </li>
          <li>
            The clock shows at once. For the visualizer, the lyrics and the player, play
            something in the Spotify desktop app on the same PC. Playback sent to
            another device through Spotify Connect is not seen.
          </li>
        </ol>

        <Sub>macOS and Linux (preview)</Sub>
        <P>
          Version 2.1.0 adds a preview for macOS and Linux that has not yet been tried
          on real machines beyond CI. Download{" "}
          <C>Inlay-&lt;version&gt;-macos.zip</C> or{" "}
          <C>Inlay-&lt;version&gt;-linux-x86_64.tar.gz</C> from the same release.
        </P>
        <P>
          It draws the clock, the player, the lyrics and the visualizer from the same
          config.toml format. The file is{" "}
          <C>~/Library/Application Support/inlay/config.toml</C> on macOS and{" "}
          <C>~/.config/inlay/config.toml</C> on Linux, and it reloads within 2 seconds
          of saving. Spotify is read through AppleScript on macOS, which asks for
          permission once, or through MPRIS on Linux.
        </P>
        <P>
          The visualizer listens to the system&apos;s default input. Set that to a
          monitor of your speakers (PulseAudio or PipeWire), or to a loopback device
          such as BlackHole.
        </P>
        <P>
          A tray or menu-bar icon offers Open config file, Open fonts folder and Quit;
          GNOME needs the AppIndicator extension to show it. The preview has no
          settings panel, edit layout, hotkeys, start at login, vertical text or
          letter spacing. Wayland may ignore window positions, and GNOME cannot keep
          widgets below windows.
        </P>
        <P>
          Inlay.app is ad-hoc signed and not notarized. The first time, open System
          Settings, then Privacy &amp; Security, and choose{" "}
          <strong className="text-foreground">Open Anyway</strong>.
        </P>

        <Sub>Checking a download</Sub>
        <P>
          Each release carries a <C>SHA256SUMS.txt</C> and a build provenance
          attestation. The second proves the file was built by this repository&apos;s
          release workflow at that tag.
        </P>
        <CodeBlock
          title="Verify the attestation"
          code="gh attestation verify Inlay-<version>.exe --repo akshit-bansal11/inlay"
        />
        <P>
          That is for 2.1.0 and later. For 2.0.0 and earlier, use{" "}
          <C>--repo akshit-bansal11/deskbeat</C> instead.
        </P>
      </Section>

      <Section
        id="using"
        eyebrow="Start here"
        title="Using it"
        lede={
          <>
            Inlay lives in the notification area. Double-click its icon for settings, or
            right-click it for a small menu.
          </>
        }
      >
        <P>
          Double-click the tray icon, or press <C>Ctrl+Alt+S</C>, for settings. The
          progress bar seeks where you click it, and the three buttons are previous,
          play/pause and next.
        </P>

        <Sub>The tray menu</Sub>
        <P>
          A right-click on the tray icon opens a small menu: the name and version, a
          link to the repository on GitHub, the <strong>Start with Windows</strong>{" "}
          switch and <strong>Quit</strong>. The app draws it itself, in its own three
          colours. Escape, or a click anywhere else, closes it. The settings panel has
          the switch and Quit too.
        </P>

        <Sub>Where the widgets sit</Sub>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Layer</TableHead>
              <TableHead>In config.toml</TableHead>
              <TableHead>What it means</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {LAYERS.map((layer) => (
              <TableRow key={layer.value}>
                <TableCell className="font-bold whitespace-nowrap">
                  {layer.name}
                </TableCell>
                <TableCell>
                  <C>{layer.value}</C>
                </TableCell>
                <TableCell className="text-dim">{layer.body}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <P>
          By default the player, the lyrics and the visualizer hide while Spotify is not
          running, and drawing stops while a fullscreen app or game is in front. Both
          are settings. Only one copy of Inlay runs at a time: a second launch hands
          over to the first and exits.
        </P>
      </Section>

      <Section
        id="edit-layout"
        eyebrow="Start here"
        title="Edit layout"
        lede={
          <>
            <C>Ctrl+Alt+E</C> turns edit layout on and off. The clock and the player
            have no box, so every one of their elements is placed on its own.
          </>
        }
      >
        <P>
          In edit layout the day, time and date, and the player&apos;s art, title,
          artist, each button, the progress bar and both times can each be dragged
          anywhere; the one under the pointer is outlined. Nothing else is drawn in edit
          layout: no frames and no labels.
        </P>
        <P>
          Every element moves alone. Shift+click selects several, which then drag
          together. The space between elements does nothing, so nothing moves as a group
          unless you select it. The lyrics and the visualizer keep a frame, because it
          is what sizes them: drag the widget to move it.
        </P>
        <P>
          The Player tab has two ready-made arrangements to start from,{" "}
          <strong>Art on the left</strong> and <strong>Centred stack</strong>, and the
          Clock tab can reset its three positions.
        </P>
        <Note label="Changed in 1.1.0">
          In 1.0.0 and 1.0.1, dragging the space between a widget&apos;s elements moves
          all of that widget. From 1.1.0 every element moves alone, and dragging one no
          longer makes the others jump or shimmer.
        </Note>
      </Section>

      <Section
        id="settings"
        eyebrow="Start here"
        title="Settings"
        lede={
          <>
            The settings panel covers the common choices: four one-click looks, fonts,
            colours, sizes, which widgets show, where they sit, and how the visualizer
            moves.
          </>
        }
      >
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Tab</TableHead>
              <TableHead>What it covers</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {PANEL.map((row) => (
              <TableRow key={row.tab}>
                <TableCell className="font-bold whitespace-nowrap">{row.tab}</TableCell>
                <TableCell className="text-dim min-w-[20rem] leading-relaxed">
                  {row.covers}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <P>
          Each widget also has a placement: the screen corner or edge it is measured
          from, so a layout survives a resolution change, its width and height, and its
          opacity.
        </P>
        <P>
          Every colour setting has a picker: the last chip in its row opens a colour
          square, a hue bar and a hex field you can type into. A one-click look only
          touches appearance, never positions.
        </P>

        <Sub>Text that runs down or up</Sub>
        <P>
          Any text of the clock and the player can run across, down or up, with its
          letters upright or lying sideways. So the day can be a column of upright
          letters, or the title can read up the side of the album art. In the panel
          these are the <strong>Runs</strong> row (Across, Down, Up) and the{" "}
          <strong>Letters</strong> row (Upright, Sideways); in the file,{" "}
          <C>direction</C> and <C>letters</C>.
        </P>

        <Sub>Fonts</Sub>
        <P>
          To use a font without installing it in Windows, put its <C>.ttf</C> or{" "}
          <C>.otf</C> file in <C>%APPDATA%\inlay\fonts</C> and restart the app. The
          clock&apos;s day and time default to Anurati and Quicksand, the pairing the
          Mond Rainmeter skin uses; they are not shipped with the app, and the theme
          font is used until they are in that folder.
        </P>
      </Section>

      <Section
        id="config"
        eyebrow="Start here"
        title="config.toml"
        lede={
          <>
            Everything is stored in one file, which you can also edit by hand. It
            reloads when you save.
          </>
        }
      >
        <CodeBlock title="The settings file" code={"%APPDATA%\\inlay\\config.toml"} />
        <P>
          Loading lays the file over the default config table by table, so a partial
          file keeps every default it does not mention, and unknown keys are ignored.
          Keys that were renamed are carried over, and every number is clamped, so a
          hand-edited or older file cannot put the app in a state the settings panel
          could not.
        </P>
        <P>
          The file has six tables: <C>[general]</C>, <C>[theme]</C>, <C>[clock]</C>,{" "}
          <C>[player]</C>, <C>[lyrics]</C> and <C>[visualizer]</C>. Colours are{" "}
          <C>#RRGGBB</C> or <C>#RRGGBBAA</C>; a text or bar colour can also be{" "}
          <C>&quot;text&quot;</C> or <C>&quot;accent&quot;</C>. These are the keys most
          worth knowing; the{" "}
          <ExternalLink href={`${REPO}/blob/main/crates/core/src/config.rs`}>
            schema
          </ExternalLink>{" "}
          has every one, with its default.
        </P>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Key</TableHead>
              <TableHead>Default</TableHead>
              <TableHead>What it does</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {CONFIG.map((row) => (
              <TableRow key={row.key}>
                <TableCell className="font-mono text-[0.8125rem] whitespace-nowrap">
                  {row.key}
                </TableCell>
                <TableCell className="font-mono text-[0.8125rem]">
                  {row.value}
                </TableCell>
                <TableCell className="text-dim min-w-[16rem] leading-relaxed">
                  {row.means}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>

        <Sub>What the file takes that the panel does not</Sub>
        <P>
          Any installed font, and a hand-written clock format such as <C>%H:%M</C>,{" "}
          <C>%l:%M %p</C> or <C>%A, %e %B</C>. Anything that is not one of these
          specifiers is left as written.
        </P>
        <Tabs defaultValue="formats">
          <TabsList>
            <TabsTrigger value="formats">Clock formats</TabsTrigger>
            <TabsTrigger value="example">An example</TabsTrigger>
          </TabsList>
          <TabsContent value="formats">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Specifier</TableHead>
                  <TableHead>Shows</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {CLOCK_FORMATS.map((row) => (
                  <TableRow key={row.spec}>
                    <TableCell className="font-mono text-[0.8125rem] whitespace-pre">
                      {row.spec}
                    </TableCell>
                    <TableCell className="text-dim">{row.shows}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </TabsContent>
          <TabsContent value="example">
            <CodeBlock
              title="A partial config.toml"
              code={`[general]
layer = "top"

[clock]
time_format = "%l:%M %p"

[clock.day]
size = 40.0
uppercase = true
direction = "down"

[player.title]
direction = "up"
letters = "sideways"

[lyrics]
mode = "progress"
offset_ms = 150

[visualizer]
style = "mirror"
bars = 96`}
              caption="Only what differs from the defaults needs to be in the file. Everything it does not mention keeps its default."
            />
          </TabsContent>
        </Tabs>
      </Section>

      <Section
        id="hotkeys"
        eyebrow="Start here"
        title="Hotkeys"
        lede={
          <>Five global hotkeys, all on Ctrl+Alt. They can be turned off in settings.</>
        }
      >
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Hotkey</TableHead>
              <TableHead>Does</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {HOTKEYS.map((row) => (
              <TableRow key={row.keys}>
                <TableCell className="font-mono text-[0.8125rem] whitespace-nowrap">
                  {row.keys}
                </TableCell>
                <TableCell className="text-dim min-w-[16rem]">{row.does}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <P>
          A hotkey that another app has already taken is left to that app; Inlay writes
          a line to its log and carries on. The switch is{" "}
          <strong>Global hotkeys</strong> on the General tab, or <C>hotkeys = false</C>{" "}
          under <C>[general]</C>.
        </P>
      </Section>

      <Section
        id="troubleshooting"
        eyebrow="Start here"
        title="If a widget stays empty"
        lede={
          <>Inlay has no console, so its diagnostics write to a file. In a terminal:</>
        }
      >
        <CodeBlock
          title="Record what Windows reports"
          code="Inlay-<version>.exe --probe > probe.txt"
          caption="With Spotify playing, this records for eight seconds what Windows reports about the track and how loud Spotify's audio is."
        />
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            <C>--probe-system</C> listens to everything the PC plays instead of Spotify
            alone.
          </li>
          <li>
            <C>--probe-lyrics &quot;Title&quot; &quot;Artist&quot;</C> looks one track
            up, for word timing and on LRCLIB.
          </li>
        </ul>
        <P>
          Errors are logged to <C>%LOCALAPPDATA%\inlay\inlay.log</C>. The lyrics cache
          is in the same folder and is safe to delete.
        </P>
        <P>
          If word-by-word lyrics stop while line-by-line ones still work, the
          community-run servers are the first thing to check: they come and go, which is
          why the list is a setting.
        </P>
      </Section>
    </>
  );
}
