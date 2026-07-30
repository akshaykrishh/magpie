import { createMDX } from 'fumadocs-mdx/next';

const withMDX = createMDX();

// GitHub Pages serves a project site (not a user/org site) at
// <user>.github.io/<repo>/, so every path needs the /magpie prefix in that
// deployment -- but only there. Local dev and any future custom-domain
// deploy should stay at the root, so this is opt-in via an env var the CI
// workflow sets, not baked in unconditionally.
const basePath = process.env.GITHUB_PAGES === 'true' ? '/magpie' : '';

/** @type {import('next').NextConfig} */
const config = {
  output: 'export',
  reactStrictMode: true,
  basePath,
  // Next.js's image optimization needs a running server to resize images
  // on request -- unavailable for a static export, which is why this is
  // required here, not optional.
  images: { unoptimized: true },
};

export default withMDX(config);
