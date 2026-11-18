import type { Metadata } from 'next';
import { Inter } from 'next/font/google';
import type { FC, ReactNode } from 'react';
import Footer from '@/components/molecules/Footer';
import Navbar from '@/components/molecules/Navbar';
import './globals.css';

const inter = Inter({ subsets: ['latin'] });

const GITHUB = 'https://github.com/hieudoanm/monkey';
const LOGO =
  process.env.NODE_ENV === 'production' ? '/monkey/logo.svg' : '/logo.svg';

const NAV_LINKS = [
  { label: 'Features', href: '#features' },
  { label: 'CLI', href: '#cli' },
  { label: 'Extensions', href: '#extensions' },
  { label: 'Install', href: '#install' },
  { label: 'Docs', href: '/docs/' },
];

const DESCRIPTION =
  'The monkey toolchain for Garmin Connect IQ: a zero-dependency Rust CLI for formatting, linting, and checking Monkey C and Jungle files, plus VS Code and Zed extensions with syntax highlighting, formatting, themes, and simulator preview.';

export const metadata: Metadata = {
  metadataBase: new URL('https://hieudoanm.github.io/monkey/'),
  title: {
    default: 'Monkey — Monkey C toolchain',
    template: '%s — Monkey',
  },
  description: DESCRIPTION,
  keywords: [
    'Monkey C',
    'Jungle',
    'Connect IQ',
    'Garmin',
    'toolchain',
    'formatter',
    'linter',
    'VS Code extension',
    'Zed extension',
  ],
  authors: [{ name: 'hieudoan', url: GITHUB }],
  category: 'developer tools',
  openGraph: {
    siteName: 'Monkey — Monkey C toolchain',
    type: 'website',
    title: 'Monkey — Monkey C toolchain',
    description: DESCRIPTION,
    url: 'https://hieudoanm.github.io/monkey/',
    images: ['/og.png'],
  },
  twitter: { card: 'summary_large_image', images: ['/og.png'] },
  icons: {
    icon: [
      { url: '/favicon.ico', sizes: 'any' },
      { url: '/favicon.svg', type: 'image/svg+xml' },
    ],
  },
};

const JSON_LD = {
  '@context': 'https://schema.org',
  '@graph': [
    {
      '@type': 'WebSite',
      name: 'Monkey — Monkey C toolchain',
      url: 'https://hieudoanm.github.io/monkey/',
      description: DESCRIPTION,
    },
    {
      '@type': 'SoftwareApplication',
      name: 'monkey',
      applicationCategory: 'DeveloperApplication',
      operatingSystem: 'Linux, macOS, Windows',
      description: DESCRIPTION,
      url: 'https://hieudoanm.github.io/monkey/',
      codeRepository: GITHUB,
      offers: { '@type': 'Offer', price: '0', priceCurrency: 'USD' },
    },
  ],
};

interface RootLayoutProps {
  children: ReactNode;
}

const RootLayout: FC<RootLayoutProps> = ({ children }) => (
  <html
    lang="en"
    data-theme="monkey"
    className={`scroll-smooth antialiased motion-reduce:scroll-auto ${inter.className}`}>
    <body className="bg-base-100 text-base-content m-0 leading-relaxed">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(JSON_LD) }}
      />
      <Navbar
        name="Monkey"
        logoSrc={LOGO}
        links={NAV_LINKS}
        githubUrl={GITHUB}
      />
      {children}
      <Footer
        name="monkey"
        tagline="Monkey C toolchain and editor extensions for Garmin Connect IQ"
        githubUrl={GITHUB}
      />
    </body>
  </html>
);

export default RootLayout;
