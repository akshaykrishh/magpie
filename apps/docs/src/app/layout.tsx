import type { Metadata } from 'next';
import { Inter, JetBrains_Mono, Space_Grotesk } from 'next/font/google';
import { Provider } from '@/components/provider';
import './global.css';

export const metadata: Metadata = {
  metadataBase: new URL('https://akshaykrishh.github.io/magpie/'),
  title: {
    default: 'magpie',
    template: '%s | magpie',
  },
  description:
    'A capture tool for AI-assisted work that runs on macOS and Linux, with an MCP server so agents can read and write your queue directly.',
};

const inter = Inter({
  subsets: ['latin'],
  variable: '--font-sans',
});

// Headings and the wordmark use Space Grotesk; body text stays on Inter,
// matching the desktop app's brand spec (Space Grotesk is a display/heading
// face there too, not a paragraph face).
const spaceGrotesk = Space_Grotesk({
  subsets: ['latin'],
  weight: ['500', '700'],
  variable: '--font-heading',
});

// Code blocks and inline code -- mapped to Tailwind's font-mono, so this
// applies anywhere Fumadocs already uses that utility without further
// wiring.
const jetbrainsMono = JetBrains_Mono({
  subsets: ['latin'],
  variable: '--font-mono',
});

export default function Layout({ children }: LayoutProps<'/'>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${spaceGrotesk.variable} ${jetbrainsMono.variable} ${inter.className}`}
      suppressHydrationWarning
    >
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
