import Link from "next/link";
import { Mark } from "@/components/mark";
import { REPO } from "@/lib/nav";

const LINK =
  "text-dim hover:text-foreground flex min-h-11 items-center rounded-lg transition-colors";

/** Shared by the landing page and the documentation page. */
export function SiteFooter() {
  return (
    <footer className="border-line mt-16 border-t">
      <div className="mx-auto flex max-w-[90rem] flex-col gap-4 px-4 py-8 text-[0.9375rem] md:flex-row md:items-center md:justify-between md:px-6">
        <div>
          <p className="flex items-center gap-2 text-lg font-black">
            <Mark className="size-6" />
            Deskbeat
          </p>
          <p className="text-dim mt-1">
            MIT licensed. Not affiliated with or endorsed by Spotify.
          </p>
        </div>
        <div className="flex flex-wrap gap-x-5">
          <Link href="/docs" className={LINK}>
            Documentation
          </Link>
          <a href={REPO} target="_blank" rel="noreferrer noopener" className={LINK}>
            GitHub
          </a>
          <a
            href={`${REPO}/blob/main/CHANGELOG.md`}
            target="_blank"
            rel="noreferrer noopener"
            className={LINK}
          >
            Changelog
          </a>
          <a
            href={`${REPO}/blob/main/SECURITY.md`}
            target="_blank"
            rel="noreferrer noopener"
            className={LINK}
          >
            Security
          </a>
          <a href="#top" className={LINK}>
            Back to top
          </a>
        </div>
      </div>
    </footer>
  );
}
