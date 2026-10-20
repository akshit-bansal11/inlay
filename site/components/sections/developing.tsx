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
import { REPO } from "@/lib/nav";

const LAYOUT: { path: string; what: string; pure?: boolean }[] = [
  {
    path: "crates/core",
    what: "Pure logic with no Windows types: lyric parsing and timing, the sync clock, band mapping, colour, the config schema. Its tests run anywhere.",
    pure: true,
  },
  {
    path: "crates/app",
    what: "The exe: windows, Direct2D drawing, the media session, audio capture, the settings panel.",
  },
  { path: "scripts/", what: "The quality gate." },
  { path: "docs/", what: "The architecture document." },
  { path: "packaging/", what: "The winget manifest." },
  { path: "site/", what: "This website." },
];

const STEPS = (
  <>
    <li>
      Bring your fork&apos;s <C>main</C> up to date with <C>upstream/main</C>, then
      branch from it on your fork.
    </li>
    <li>
      Keep <C>crates/core</C> free of Windows types. Anything that needs a window, a
      device or Spotify goes in <C>crates/app</C>.
    </li>
    <li>
      A new setting is four things in one commit: the field with its default, its clamp
      in <C>Config::sanitized</C>, a row in the settings panel, and a test if an older
      config file has to keep loading.
    </li>
    <li>
      Add or change a test when you add logic, a branch or fix a bug. Prefer testing in{" "}
      <C>crates/core</C> over anything that needs a window.
    </li>
    <li>
      Run <C>pwsh scripts/check.ps1</C> until it is clean. Do not suppress a lint
      finding to get green.
    </li>
    <li>
      After changing a <C>Cargo.toml</C>, commit the <C>Cargo.lock</C> that <C>cargo</C>{" "}
      rewrites. The gate uses <C>--locked</C> and fails on a stale one.
    </li>
    <li>
      Commit with a Conventional Commits subject (<C>feat:</C>, <C>fix:</C>,{" "}
      <C>docs:</C>, <C>ci:</C>, with an optional scope such as <C>feat(lyrics):</C>) and
      a body that says what changed and why, including what you rejected.
    </li>
    <li>
      Add a line under <strong className="text-foreground">Unreleased</strong> in{" "}
      <C>CHANGELOG.md</C>.
    </li>
    <li>
      Push the branch to your fork and open a pull request against <C>main</C>. CI runs
      the gate, builds the exe and renders the snapshots. The maintainer reviews and
      merges.
    </li>
  </>
);

const STYLE = (
  <>
    <li>
      No new dependency without a reason in the pull request. The whole workspace has
      seven.
    </li>
    <li>
      The <C>windows</C> crate stays on 0.62. Its API moves between minor versions; a
      bump is its own pull request.
    </li>
    <li>Constants are named. A number that appears twice is a constant.</li>
    <li>Doc comments say why, not what the next line already says.</li>
    <li>
      A widget that asks for a frame while nothing on it is moving is a bug. It is the
      whole reason this app exists instead of a Rainmeter skin.
    </li>
  </>
);

export function DevelopingSections() {
  return (
    <>
      <Section
        id="repo"
        eyebrow="Developing"
        title="Repository layout"
        lede={
          <>
            The code is MIT-licensed, so you can build it, change it and run your own
            copy. To send a change back, fork the repository on GitHub, then clone your
            fork.
          </>
        }
      >
        <CodeBlock
          title="Clone your fork"
          code={`git clone https://github.com/<you>/inlay.git inlay
cd inlay
git remote add upstream https://github.com/akshit-bansal11/inlay.git`}
          caption="The upstream remote is there to pull in later changes."
        />
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Path</TableHead>
              <TableHead>What</TableHead>
              <TableHead>Needs Windows</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {LAYOUT.map((row) => (
              <TableRow key={row.path}>
                <TableCell className="font-mono text-[0.8125rem] whitespace-nowrap">
                  {row.path}
                </TableCell>
                <TableCell className="text-dim min-w-[18rem] leading-relaxed">
                  {row.what}
                </TableCell>
                <TableCell>
                  {row.path.startsWith("crates/") ? (
                    <Badge variant={row.pure ? "solid" : "outline"}>
                      {row.pure ? "no" : "yes"}
                    </Badge>
                  ) : null}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Section>

      <Section
        id="prerequisites"
        eyebrow="Developing"
        title="Prerequisites"
        lede={<>What a machine needs before the app will build.</>}
      >
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            Windows 11 for the Windows app, or macOS or Linux for the preview in{" "}
            <C>crates/desk</C>.
          </li>
          <li>
            The stable Rust toolchain, 1.89 or later, with <C>rustfmt</C> and{" "}
            <C>clippy</C>, and the MSVC build tools that <C>rustup</C> asks for on
            Windows.
          </li>
          <li>
            On Linux, the packages <C>libasound2-dev</C>, <C>libdbus-1-dev</C>,{" "}
            <C>libxkbcommon-dev</C> and <C>pkg-config</C>, and the xcb development
            packages.
          </li>
          <li>
            PowerShell 7 (<C>pwsh</C>), which runs the gate.
          </li>
        </ul>
        <Sub>Where to start without Spotify</Sub>
        <P>
          <C>crates/core</C> has no Windows types at all, and holds most of the logic
          that matters: lyric parsing and timing, the sync clock, band mapping, colour,
          the config schema and its migrations. Its tests run anywhere, in milliseconds.
        </P>
        <P>
          <C>crates/app</C> needs Windows. The snapshot mode draws every widget from
          made-up data, with no GPU, no Spotify and no audio device. What it cannot show
          is anything in the app loop: dragging, the settings panel&apos;s pointer
          handling, the tray, live reload. Those need a real run.
        </P>
      </Section>

      <Section
        id="gate"
        eyebrow="Developing"
        title="The quality gate"
        lede={
          <>
            One script formats, lints and tests; CI runs the same script in its
            non-mutating mode, so the local gate and CI cannot disagree.
          </>
        }
      >
        <CodeBlock
          title="The quality gate"
          code={`pwsh scripts/check.ps1        # formats in place, then clippy (warnings are errors) and tests
pwsh scripts/check.ps1 -Ci    # what CI runs: fails on unformatted code instead of fixing it`}
        />
        <P>
          It writes locally because fixing your formatting is useful; CI must not write
          to your branch, so it fails instead. Clippy and the tests run with{" "}
          <C>--locked</C>: a gate that quietly rewrites <C>Cargo.lock</C> has checked a
          different build.
        </P>
        <Note label="This site has its own gate">
          <C>scripts/check.ps1</C> checks only the app. The site under <C>site/</C> is
          checked by <C>npm run check</C> there (Biome, ESLint, then tsc), which a
          separate workflow runs only when the site changes.
        </Note>
      </Section>

      <Section
        id="run-locally"
        eyebrow="Developing"
        title="Running it locally"
        lede={<>Build and run directly, without the gate, while iterating.</>}
      >
        <CodeBlock
          title="The tray app"
          code="cargo run --release"
          caption="Only one copy runs at a time. If a release build is already in your tray, quit it first, or the new one hands over to it and exits."
        />
        <CodeBlock
          title="The macOS and Linux preview"
          code="cargo run -p inlay-desk --release"
        />
        <CodeBlock
          title="Every widget, drawn to PNG"
          code="cargo run --release -- --snapshot snapshots"
          caption="Renders every widget and the settings panel from made-up data, with no GPU or Spotify. It is how CI checks the drawing code, and the quickest way to see a visual change."
        />
        <CodeBlock title="The tests alone" code="cargo test --workspace" />
      </Section>

      <Section
        id="tests"
        eyebrow="Developing"
        title="Tests"
        lede={<>Most of them live where they can run without a window.</>}
      >
        <P>
          <C>crates/core</C> carries most of them: the LRC parser on real, half-broken
          files; word timing and its invariants; the sync clock against drift, jitter,
          seeks and pauses; title cleaning; config defaults, clamps and migrations from
          older files; band mapping; colour.
        </P>
        <P>
          <C>crates/app</C> tests what can be tested without a window: sample mixing,
          the lyric widget&apos;s place-in-the-song logic, the settings panel&apos;s hex
          field, the tray icon&apos;s pixels.
        </P>
        <P>
          <C>inlay.exe --snapshot &lt;dir&gt;</C> renders every widget and the settings
          panel to PNG from made-up data, with no GPU, Spotify or audio device. CI runs
          it on every push. It proves the drawing code runs and lets a change be looked
          at; it does not exercise the app loop, a pointer, or a live session.
        </P>
      </Section>

      <Section
        id="contributing"
        eyebrow="Developing"
        title="Contributing"
        lede={
          <>
            Inlay is small on purpose; the bar for a change is that it makes the product
            better for someone using it, and that it passes the same gate CI runs.
          </>
        }
      >
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          <li>
            <strong className="text-foreground">
              A bug, or something Inlay should do:
            </strong>{" "}
            <ExternalLink href={`${REPO}/issues/new/choose`}>
              open an issue
            </ExternalLink>
            . Open one before starting anything bigger than a fix, so the design can be
            talked through before code exists.
          </li>
          <li>
            <strong className="text-foreground">A security hole:</strong> report it
            privately, as the security section above describes, not in a public issue.
          </li>
          <li>
            <strong className="text-foreground">A change:</strong> make it on a branch
            of your fork, run the gate until it is clean, and open a pull request
            against <C>main</C>.
          </li>
        </ul>

        <Sub>Making a change</Sub>
        <ol className="text-dim max-w-[68ch] list-decimal space-y-3 pl-5 text-[0.9375rem] leading-relaxed">
          {STEPS}
        </ol>

        <Sub>Style</Sub>
        <ul className="text-dim max-w-[68ch] list-disc space-y-2 pl-5 text-[0.9375rem] leading-relaxed">
          {STYLE}
        </ul>
        <P>
          The full text is{" "}
          <ExternalLink href={`${REPO}/blob/main/CONTRIBUTING.md`}>
            CONTRIBUTING.md
          </ExternalLink>
          .
        </P>
      </Section>

      <Section
        id="releases"
        eyebrow="Developing"
        title="Releases"
        lede={
          <>
            The maintainer builds and publishes releases. A pull request does not bump
            the version, tag, or build a release; a merged change ships in the next one.
          </>
        }
      >
        <P>
          Pushing a <C>v*</C> tag runs the release workflow: the gate again, a release
          build, a check that the tag matches the crate version, the snapshot smoke
          test, then a GitHub Release carrying <C>Inlay-&lt;version&gt;.exe</C>,{" "}
          <C>SHA256SUMS.txt</C> and a build provenance attestation, with notes taken
          from that version&apos;s section of <C>CHANGELOG.md</C>.
        </P>
        <P>
          Running the same workflow by hand attaches the build to a private draft
          release instead. Every action in every workflow is pinned to a commit, and
          Rust comes from the runner image, not from an action.
        </P>
      </Section>
    </>
  );
}
