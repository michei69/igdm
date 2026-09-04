import { useRemoteImage } from "../../hooks/useRemoteImage";

export interface Slide {
  videoUrl: string | null;
  imageUrl: string | null;
}

interface Props {
  slides: Slide[];
  currentIndex: number;
  expired: boolean;
  onPrevious: () => void;
  onNext: () => void;
}

export default function ShareMediaCarousel({
  slides,
  currentIndex,
  expired,
  onPrevious,
  onNext,
}: Props) {
  const current = slides[Math.min(currentIndex, slides.length - 1)];
  const { src: imgSrc, onError: imgError } = useRemoteImage(current?.imageUrl);
  const multi = slides.length > 1;

  return (
    <div className="relative flex min-h-0 min-w-0 flex-1 items-center justify-center overflow-hidden rounded-lg bg-black">
      {current?.videoUrl ? (
        <video
          key={current.videoUrl}
          src={current.videoUrl}
          controls
          autoPlay
          muted
          playsInline
          className="max-h-[55vh] w-full object-contain"
        />
      ) : imgSrc ? (
        <img
          src={imgSrc}
          alt="Share preview"
          className="max-h-[55vh] w-full object-contain"
          onError={imgError}
        />
      ) : (
        <span className="p-6 text-[13px] text-ink3">Preview unavailable</span>
      )}
      {expired && (
        <span className="absolute top-2 left-2 rounded-full bg-black/60 px-2 py-0.5 text-[11px] font-medium text-white">
          Story expired
        </span>
      )}
      {multi && (
        <>
          <button
            className="absolute top-1/2 left-2 flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-full bg-black/60 text-[14px] text-white disabled:opacity-30"
            disabled={currentIndex === 0}
            onClick={onPrevious}
            aria-label="Previous photo"
          >
            ‹
          </button>
          <button
            className="absolute top-1/2 right-2 flex h-8 w-8 -translate-y-1/2 items-center justify-center rounded-full bg-black/60 text-[14px] text-white disabled:opacity-30"
            disabled={currentIndex >= slides.length - 1}
            onClick={onNext}
            aria-label="Next photo"
          >
            ›
          </button>
          <span className="absolute right-2 bottom-2 rounded-full bg-black/60 px-2 py-0.5 text-[11px] text-white">
            {currentIndex + 1} / {slides.length}
          </span>
        </>
      )}
    </div>
  );
}
