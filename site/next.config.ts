import type { NextConfig } from "next";

const config: NextConfig = {
  // Every page is prerendered and nothing is read at request time, so the site ships
  // as plain static files: no Next.js runtime, no serverless functions, and it can be
  // hosted anywhere that serves a directory.
  output: "export",

  // A static export has no image optimiser to call, and the hero is an animated GIF
  // that must be served as it is.
  images: { unoptimized: true },
};

export default config;
