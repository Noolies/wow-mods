// The app mark (anvil and hearth flame) and the dusk ridgeline banner.

export function Mark({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 1024 1024" aria-hidden="true">
      <defs>
        <linearGradient id="mark-brass" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#f2cf7a" />
          <stop offset="0.55" stopColor="#d2a145" />
          <stop offset="1" stopColor="#9c6f27" />
        </linearGradient>
        <linearGradient id="mark-ember" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0" stopColor="#ffe3a1" />
          <stop offset="1" stopColor="#ee8f3d" />
        </linearGradient>
      </defs>
      <path
        d="M532 214 C602 292 650 352 632 420 C618 474 576 498 532 498 C488 498 446 474 432 420 C420 372 448 334 482 300 C488 334 500 356 522 368 C514 322 514 270 532 214 Z"
        fill="url(#mark-ember)"
      />
      <path
        d="M150 548 C222 512 298 498 380 496 L834 496 L834 566 C770 572 708 592 668 628 L668 700 C710 710 750 732 772 772 L772 812 L320 812 L320 772 C342 732 382 710 424 700 L424 628 C384 598 332 582 270 578 C228 576 186 566 150 548 Z"
        fill="url(#mark-brass)"
      />
    </svg>
  );
}

const PINES = [70, 92, 286, 304, 322, 708, 724, 1004, 1022, 1040, 1156];

function pine(x: number, base: number, scale = 1) {
  const h = 38 * scale;
  const w = 13 * scale;
  return `M${x} ${base - h} L${x + w * 0.7} ${base - h * 0.48} L${x + w * 0.38} ${base - h * 0.48} L${x + w} ${base} L${x - w} ${base} L${x - w * 0.38} ${base - h * 0.48} L${x - w * 0.7} ${base - h * 0.48} Z`;
}

/** Layered ridges at dusk with a warm glow on the horizon. Purely decorative. */
export function Ridgeline({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 1200 220" preserveAspectRatio="xMidYMax slice" aria-hidden="true">
      <defs>
        <radialGradient id="ridge-glow" cx="0.5" cy="0.5" r="0.5">
          <stop offset="0" stopColor="var(--glow)" />
          <stop offset="1" stopColor="var(--glow)" stopOpacity="0" />
        </radialGradient>
      </defs>
      <ellipse cx="930" cy="168" rx="300" ry="120" fill="url(#ridge-glow)" />
      {[
        [120, 32, 1.2],
        [260, 18, 0.9],
        [410, 44, 1],
        [575, 26, 0.8],
        [660, 52, 1.1],
        [1110, 30, 0.9],
      ].map(([x, y, r]) => (
        <circle key={x} cx={x} cy={y} r={r} fill="var(--text)" opacity="0.35" />
      ))}
      <path
        d="M0 150 L80 118 L150 134 L240 92 L320 124 L400 104 L470 130 L560 84 L650 120 L730 98 L820 128 L900 88 L990 118 L1080 100 L1200 124 L1200 220 L0 220 Z"
        fill="var(--ridge-far)"
      />
      <path
        d="M0 176 L60 160 L130 170 L210 138 L280 164 L350 150 L430 172 L520 136 L600 168 L690 148 L770 176 L860 142 L940 168 L1020 150 L1110 172 L1200 156 L1200 220 L0 220 Z"
        fill="var(--ridge-mid)"
      />
      <path
        d="M0 200 C120 188 220 196 320 190 C430 184 520 198 640 192 C760 186 860 200 980 192 C1080 186 1150 194 1200 190 L1200 220 L0 220 Z"
        fill="var(--ridge-near)"
      />
      <path d={PINES.map((x, i) => pine(x, 196, i % 3 === 1 ? 0.75 : 1)).join(" ")} fill="var(--ridge-near)" />
    </svg>
  );
}
