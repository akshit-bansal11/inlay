import type { Metadata } from "next";
import { DevelopingSections } from "@/components/sections/developing";
import { InternalsSections } from "@/components/sections/internals";
import { ProductSections } from "@/components/sections/product";
import { ReferenceSections } from "@/components/sections/reference";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import { Toc } from "@/components/toc";
import { SOCIAL_IMAGE } from "@/lib/nav";

const TITLE = "Inlay — documentation";
const DESCRIPTION =
  "Full Inlay documentation: install, the four widgets, edit layout, settings and config.toml, hotkeys, lyrics sources, architecture, privacy, and the developer guide, on one page.";

// A page's openGraph replaces the layout's whole object rather than merging into it,
// hence type, site name and image again here.
export const metadata: Metadata = {
  title: TITLE,
  description: DESCRIPTION,
  alternates: { canonical: "/docs" },
  openGraph: {
    title: TITLE,
    description: DESCRIPTION,
    type: "website",
    url: "/docs",
    siteName: "Inlay",
    images: [SOCIAL_IMAGE],
  },
};

export default function DocsPage() {
  return (
    <div id="top" className="min-h-dvh">
      <SiteHeader variant="docs" />

      <main id="main" className="mx-auto max-w-[90rem] scroll-mt-16 px-4 md:px-6">
        {/* No entrance on the title block: it is the first paint, and a fade would
            hide it until the JavaScript arrives. */}
        <div className="py-10 md:py-16">
          <p className="text-primary-text mb-2 text-[0.9375rem] font-bold">
            Documentation
          </p>
          <h1 className="max-w-[20ch] text-[2.5rem] leading-[1.04] font-black tracking-[-0.025em] md:text-6xl">
            Everything Inlay does, and how.
          </h1>
          <p className="text-dim mt-5 max-w-[64ch] text-base leading-relaxed md:text-lg">
            Install, the widgets, edit layout, settings and the file behind them,
            hotkeys, where lyrics come from, the architecture and the developer guide.
            Every statement here traces to the repository&apos;s own documents or its
            code.
          </p>
        </div>

        {/* Sidebar + body */}
        <div className="lg:grid lg:grid-cols-[17rem_minmax(0,1fr)] lg:gap-12">
          <aside className="hidden lg:block">
            <div className="bg-card shadow-card sticky top-20 max-h-[calc(100dvh-6rem)] overflow-y-auto rounded-[var(--radius-panel)] p-3 pt-4">
              <Toc />
            </div>
          </aside>

          <div className="divide-line min-w-0 divide-y">
            <ProductSections />
            <InternalsSections />
            <DevelopingSections />
            <ReferenceSections />
          </div>
        </div>
      </main>

      <SiteFooter />
    </div>
  );
}
