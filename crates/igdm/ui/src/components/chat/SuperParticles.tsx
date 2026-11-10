// IG super-react burst: ~20 emoji particles emitted over ~200 ms inside the
// bubble rect, drifting up under gravity. Every particle's randomised geometry
// is computed once per burst and fed to CSS keyframes through custom
// properties, so the animation never touches JS per frame.

import { useMemo, type CSSProperties } from "react";
import { emojiDisplay } from "../../lib/format";
import type { SuperBurst } from "./MessageRow";

const PARTICLE_COUNT = 20;
/** Emission window: one particle every 10 ms. */
const EMIT_INTERVAL_MS = 10;
/** Base emoji size; per-particle scale multiplies it 0.36–1.2x. */
const PARTICLE_SIZE_PX = 24;

/** CSS custom properties accepted alongside the standard style props. */
type ParticleStyle = CSSProperties & Record<`--${string}`, string>;

const OVERLAY_STYLE: ParticleStyle = { "--particle-size": `${PARTICLE_SIZE_PX}px` };

function randomBetween(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

/** Randomised spawn/rotation/scale/drift for one particle. */
function particleStyle(index: number): ParticleStyle {
  const x = randomBetween(15, 85);
  const y = randomBetween(33, 93);
  const rotation = randomBetween(-36, 36);
  const scale = randomBetween(0.36, 1.2);
  // Gravity is negative (particles rise); terminal vy = gravity / 3.
  const gravity = randomBetween(-300, -150);
  const lifetime = randomBetween(800, 1200);
  // Horizontal velocity is damped quickly, so the horizontal travel is small.
  // Full translate values are precomputed so the keyframes never need to
  // multiply a custom property (keeps the CSS parseable everywhere).
  const dx = randomBetween(-22, 22);
  const rise = Math.abs((gravity / 3) * (lifetime / 1000) * 0.75);
  const tx = (progress: number) => `calc(-50% + ${(dx * progress).toFixed(1)}px)`;
  const ty = (progress: number) => `calc(-50% - ${(rise * progress).toFixed(1)}px)`;
  return {
    "--x": `${x.toFixed(1)}%`,
    "--y": `${y.toFixed(1)}%`,
    "--r": `${rotation.toFixed(1)}deg`,
    "--s": scale.toFixed(2),
    "--tx33": tx(0.7),
    "--ty33": ty(0.33),
    "--tx100": tx(1),
    "--ty100": ty(1),
    "--d": `${lifetime.toFixed(0)}ms`,
    "--delay": `${index * EMIT_INTERVAL_MS}ms`,
  };
}

export default function SuperParticles({ burst }: { burst: SuperBurst }) {
  // Keyed by burst nonce in MessageRow, so one mount = one burst's particles.
  const particles = useMemo(
    () => Array.from({ length: PARTICLE_COUNT }, (_, i) => ({ id: i, style: particleStyle(i) })),
    [burst],
  );
  return (
    <span
      className="pointer-events-none absolute inset-0 z-20"
      aria-hidden="true"
      style={OVERLAY_STYLE}
    >
      {particles.map((particle) => (
        <span key={particle.id} className="super-particle emoji-font" style={particle.style}>
          {emojiDisplay(burst.emoji)}
        </span>
      ))}
    </span>
  );
}
