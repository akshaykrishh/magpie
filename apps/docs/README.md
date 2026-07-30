# magpie docs

The docs site at [akshaykrishh.github.io/magpie](https://akshaykrishh.github.io/magpie), built
with [Fumadocs](https://fumadocs.dev) and deployed to GitHub Pages on every push that touches
this directory (see `.github/workflows/deploy-docs.yml`). Content lives in `content/docs/`.

It's a Next.js app with [Static Export](https://nextjs.org/docs/app/guides/static-exports)
configured -- `next.config.mjs` sets a `/magpie` basePath specifically for the GitHub Pages
deployment, opt-in via the `GITHUB_PAGES` env var so local dev stays at the root.

Run development server:

```bash
npm run dev
# or
pnpm dev
# or
yarn dev
```

Open http://localhost:3000 with your browser to see the result.

## Explore

In the project, you can see:

- `lib/source.ts`: Code for content source adapter, [`loader()`](https://fumadocs.dev/docs/headless/source-api) provides the interface to access your content.
- `lib/layout.shared.tsx`: Shared options for layouts, optional but preferred to keep.

| Route                     | Description                                            |
| ------------------------- | ------------------------------------------------------ |
| `app/(home)`              | The route group for your landing page and other pages. |
| `app/docs`                | The documentation layout and pages.                    |
| `app/api/search/route.ts` | The Route Handler for search.                          |

### Fumadocs MDX

A `source.config.ts` config file has been included, you can customise different options like frontmatter schema.

Read the [Introduction](https://fumadocs.dev/docs/mdx) for further details.

## Learn More

To learn more about Next.js and Fumadocs, take a look at the following
resources:

- [Next.js Documentation](https://nextjs.org/docs) - learn about Next.js
  features and API.
- [Learn Next.js](https://nextjs.org/learn) - an interactive Next.js tutorial.
- [Fumadocs](https://fumadocs.dev) - learn about Fumadocs
