import Image from "next/image";
import { BorderBeam } from "@/components/ui/border-beam";

const WIDTH = 880;
const HEIGHT = 470;
const ALT =
  "A desktop while a song plays: a spectrum moving along the top edge, lyrics on the left lighting up line by line, and the day, a progress bar, the track title and playback buttons at the bottom";

/**
 * The recording from the repository's README, 880×470. A GIF cannot be paused, so
 * under prefers-reduced-motion its first frame is shown instead. The swap is done in
 * CSS, like every other motion rule on the site: the server cannot know the
 * preference. Both are lazy, and a lazy image that is `display: none` is never
 * fetched, so only one of the two files travels.
 */
export function DemoFigure() {
  return (
    <figure>
      <div className="shadow-lift relative overflow-hidden rounded-[var(--radius-panel)] bg-black">
        <Image
          src="/demo.gif"
          alt={ALT}
          width={WIDTH}
          height={HEIGHT}
          className="block h-auto w-full motion-reduce:hidden"
        />
        <Image
          src="/demo-poster.png"
          alt={ALT}
          width={WIDTH}
          height={HEIGHT}
          className="hidden h-auto w-full motion-reduce:block"
        />
        <BorderBeam
          size={140}
          duration={9}
          borderWidth={2}
          colorFrom="var(--ember)"
          colorTo="var(--ember-soft)"
        />
      </div>
      <figcaption className="text-dim mt-3 text-sm">
        Deskbeat on a real desktop, with Spotify playing.
      </figcaption>
    </figure>
  );
}
