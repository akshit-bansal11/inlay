/**
 * The page's table of contents. One list, used three times: by the sidebar, by the
 * mobile menu, and by the scroll-spy. Every `id` here must exist as a section anchor
 * on the documentation page.
 */
export type NavItem = { id: string; label: string };
export type NavGroup = { label: string; items: NavItem[] };

export const NAV: NavGroup[] = [
  {
    label: "Start here",
    items: [
      { id: "overview", label: "What Inlay is" },
      { id: "install", label: "Download and install" },
      { id: "using", label: "Using it" },
      { id: "edit-layout", label: "Edit layout" },
      { id: "settings", label: "Settings" },
      { id: "config", label: "config.toml" },
      { id: "hotkeys", label: "Hotkeys" },
      { id: "troubleshooting", label: "If a widget stays empty" },
    ],
  },
  {
    label: "How it works",
    items: [
      { id: "sources", label: "Where the data comes from" },
      { id: "lyrics", label: "Lyrics sources" },
      { id: "architecture", label: "Architecture" },
      { id: "loop", label: "The loop" },
      { id: "drawing", label: "Drawing" },
      { id: "security", label: "Privacy and security" },
    ],
  },
  {
    label: "Developing",
    items: [
      { id: "repo", label: "Repository layout" },
      { id: "prerequisites", label: "Prerequisites" },
      { id: "gate", label: "The quality gate" },
      { id: "run-locally", label: "Running it locally" },
      { id: "tests", label: "Tests" },
      { id: "contributing", label: "Contributing" },
      { id: "releases", label: "Releases" },
    ],
  },
  {
    label: "Reference",
    items: [
      { id: "decisions", label: "Design decisions" },
      { id: "limits", label: "Known limits" },
      { id: "history", label: "Version history" },
      { id: "credits", label: "Licence and credits" },
    ],
  },
];

export const ALL_NAV_IDS: string[] = NAV.flatMap((group) =>
  group.items.map((item) => item.id),
);

export const REPO = "https://github.com/akshit-bansal11/inlay";
export const LATEST_RELEASE = `${REPO}/releases/latest`;

/** The site's one origin. Canonical links, the sitemap and robots.txt all resolve against it. */
export const SITE = "https://inlay.akshitbansal.dev";

/** The card a shared link unfurls into: the repository's own social preview, 1280×640. */
export const SOCIAL_IMAGE = {
  url: "/social-preview.png",
  width: 1280,
  height: 640,
  alt: "Inlay: live widgets, set into your desktop. A clock, a spectrum visualizer, synced lyrics and a now-playing player.",
};
