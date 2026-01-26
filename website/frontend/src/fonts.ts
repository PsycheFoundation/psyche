import { css } from "@linaria/core";

import souffletWoff2 from "./assets/fonts/SouffletVert-Hybrid106R.woff2";

import geistMonoWoff2 from "./assets/fonts/GeistMono[wght].woff2";

import geistWoff2 from "./assets/fonts/Geist[wght].woff2";

export const global = css`
  :global() {
    @font-face {
      font-family: "Soufflet Vert Hybrid 106R";
      src: url("${souffletWoff2}") format("woff2");
      font-weight: normal;
      font-style: normal;
      font-display: swap;
    }

    @font-face {
      font-family: "Geist Mono";
      src: url("${geistMonoWoff2}") format("woff2");
      font-weight: 1 999;
      font-style: normal;
      font-display: swap;
    }

    @font-face {
      font-family: "Geist";
      src: url("${geistWoff2}") format("woff2");
      font-weight: 1 999;
      font-style: normal;
      font-display: swap;
    }
  }
`;

const font = {
  display: css`
    font-family: "Soufflet Vert Hybrid 106R";
    text-transform: lowercase;
  `,
  headline: css`
    font-family: "Geist Mono";
  `,
  body: css`
    font-family: "Geist Mono";
    letter-spacing: -0.25px;
  `,
  aux: css`
    font-family: "Geist";
  `,
  button: css`
    font-family: "Geist Mono";
    letter-spacing: 0.5px;
  `,
};

const size = {
  xs: css`
    font-size: 0.75rem;
    line-height: 1rem;
  `,
  sm: css`
    font-size: 0.875rem;
    line-height: 1.25rem;
  `,
  base: css`
    font-size: 1rem;
    line-height: 1.5rem;
  `,
  lg: css`
    font-size: 1.125rem;
    line-height: 1.75rem;
  `,
  xl: css`
    font-size: 1.25rem;
    line-height: 1.75rem;
  `,
  "2xl": css`
    font-size: 1.5rem;
    line-height: 2rem;
  `,
  "3xl": css`
    font-size: 1.875rem;
    line-height: 2.25rem;
  `,
  "4xl": css`
    font-size: 2.25rem;
    line-height: 2.5rem;
  `,
  "5xl": css`
    font-size: 3rem;
    line-height: 1;
  `,
  "6xl": css`
    font-size: 3.75rem;
    line-height: 1;
  `,
  "7xl": css`
    font-size: 4.5rem;
    line-height: 1;
  `,
};

const weight = {
  regular: css`
    font-weight: 400;
  `,
  medium: css`
    font-weight: 500;
  `,
  semibold: css`
    font-weight: 600;
  `,
  bold: css`
    font-weight: 700;
  `,
};

type FontKey = keyof typeof font;
type SizeKey = keyof typeof size;
type WeightKey = keyof typeof weight;

const fontConfig = {
  display: {
    sizes: ["2xl", "3xl", "4xl", "5xl", "6xl", "7xl"] as const,
  },
  headline: {
    sizes: ["2xl", "3xl", "4xl", "5xl"] as const,
    weights: ["regular", "semibold", "bold"] as const,
  },
  body: {
    sizes: ["xs", "sm", "base", "lg", "xl"] as const,
    weights: ["regular", "medium", "semibold"] as const,
  },
  aux: {
    sizes: ["xs", "sm", "base", "lg", "xl"] as const,
    weights: ["regular", "medium", "semibold", "bold"] as const,
  },
  button: {
    sizes: ["sm", "base", "lg", "xl"] as const,
  },
} as const satisfies Record<
  FontKey,
  {
    sizes: SizeKey[];
    weights?: WeightKey[];
  }
>;

type FontClassNames = {
  [K in FontKey as `${K}/${(typeof fontConfig)[K]["sizes"][number]}${(typeof fontConfig)[K] extends {
    weights: readonly WeightKey[];
  }
    ? `/${(typeof fontConfig)[K]["weights"][number]}`
    : ""}`]: string;
};

function generateFontClasses(): FontClassNames {
  const classes: Record<string, string> = {};

  Object.entries(fontConfig).forEach(([fontName, config]) => {
    const fontClass = font[fontName as FontKey];

    config.sizes.forEach((sizeKey) => {
      const sizeClass = size[sizeKey as SizeKey];

      if ("weights" in config) {
        config.weights.forEach((weightKey) => {
          const weightClass = weight[weightKey as WeightKey];
          const key = `${fontName}/${sizeKey}/${weightKey}`;
          classes[key] = `${fontClass} ${sizeClass} ${weightClass}`.trim();
        });
      } else {
        const key = `${fontName}/${sizeKey}`;
        classes[key] = `${fontClass} ${sizeClass}`.trim();
      }
    });
  });

  return classes as FontClassNames;
}

export const text = generateFontClasses();
