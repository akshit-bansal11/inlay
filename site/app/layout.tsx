import type { Metadata, Viewport } from "next";
import { JetBrains_Mono, Lato } from "next/font/google";
import { ThemeProvider } from "@/components/theme-provider";
import { SITE, SOCIAL_IMAGE } from "@/lib/nav";
import "./globals.css";

/*
  Every piece of text is Lato, in three weights: 400 for reading, 700 for labels and
  buttons, 900 for titles. JetBrains Mono stays for what really is code: commands,
  config keys, file paths. Both are downloaded by next/font at build and served from
  this origin, so the page asks no other host for anything.
*/
const sans = Lato({
  subsets: ["latin"],
  weight: ["400", "700", "900"],
  variable: "--font-sans-family",
  display: "swap",
});

const mono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-mono-family",
  display: "swap",
});

const TITLE = "Inlay — live widgets, set into your desktop";

export const metadata: Metadata = {
  // Every relative URL in metadata (Open Graph, canonical) resolves against this.
  metadataBase: new URL(SITE),
  // The landing page's title is the default; app/docs/page.tsx overrides it. The
  // canonical link is each page's own, so a future page never inherits "/" by accident.
  title: TITLE,
  description:
    "Inlay sets five live widgets into the desktop on Windows 11, with a preview for macOS and Linux: a clock, a spectrum visualizer, time-synced lyrics and a now-playing player that follow Spotify, and a System widget for CPU, RAM, GPU, drives, network and battery. One small native app, a light alternative to Rainmeter. No Spotify login, no telemetry. MIT.",
  applicationName: "Inlay",
  authors: [{ name: "akshit-bansal11" }],
  openGraph: {
    title: TITLE,
    description:
      "A clock, a spectrum visualizer, synced lyrics and a now-playing player, set into the desktop on Windows 11, with a preview for macOS and Linux. They redraw only when something changes. No Spotify login.",
    type: "website",
    url: "/",
    siteName: "Inlay",
    images: [SOCIAL_IMAGE],
  },
  twitter: {
    card: "summary_large_image",
    images: [SOCIAL_IMAGE],
  },
};

export const viewport: Viewport = {
  themeColor: [
    { media: "(prefers-color-scheme: light)", color: "#f3efe6" },
    { media: "(prefers-color-scheme: dark)", color: "#15151c" },
  ],
};

export default function RootLayout({
  children,
}: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body className={`${mono.variable} ${sans.variable} font-sans`}>
        <ThemeProvider
          attribute="class"
          defaultTheme="system"
          enableSystem
          disableTransitionOnChange
        >
          {children}
        </ThemeProvider>
      </body>
    </html>
  );
}
