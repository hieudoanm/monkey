import type { Metadata } from 'next';
import type { FC } from 'react';
import type { IconType } from 'react-icons';
import {
  FaApple,
  FaDownload,
  FaLinux,
  FaPuzzlePiece,
  FaTerminal,
  FaWindows,
} from 'react-icons/fa';

export const metadata: Metadata = {
  title: 'Downloads',
  description:
    'Download the Monkey C toolchain CLI and editor extensions for Garmin Connect IQ development.',
  alternates: { canonical: '/downloads/' },
};

const GITHUB = 'https://github.com/hieudoanm/monkey';
const RELEASES = `${GITHUB}/releases/latest/download`;
const RELEASE_PAGE = `${GITHUB}/releases/latest`;

interface Asset {
  label: string;
  href: string;
}

interface Platform {
  name: string;
  icon: IconType;
  assets: Asset[];
}

const CLI: Platform[] = [
  {
    name: 'macOS',
    icon: FaApple,
    assets: [
      { label: 'Apple Silicon', href: `${RELEASES}/cli-monkey-aarch64-apple-darwin` },
    ],
  },
  {
    name: 'Windows',
    icon: FaWindows,
    assets: [
      { label: '.exe', href: `${RELEASES}/cli-monkey-x86_64-pc-windows-msvc.exe` },
    ],
  },
  {
    name: 'Linux',
    icon: FaLinux,
    assets: [
      { label: 'x86_64', href: `${RELEASES}/cli-monkey-x86_64-unknown-linux-gnu` },
    ],
  },
];

const EXTENSIONS: Asset[] = [
  {
    label: 'VS Code extension (.vsix)',
    href: `${RELEASES}/extension-monkey-c.vsix`,
  },
  {
    label: 'Zed extension (.zip)',
    href: `${RELEASES}/extension-monkey-c-zed.zip`,
  },
];

function PlatformGrid({ platforms }: { platforms: Platform[] }) {
  return (
    <div className="grid grid-cols-1 gap-6 sm:grid-cols-3">
      {platforms.map((p) => (
        <div key={p.name} className="card border-base-300 bg-base-100 border p-6">
          <div className="bg-primary/10 text-primary mb-3.5 flex h-11 w-11 items-center justify-center rounded-lg">
            <p.icon className="h-6 w-6" />
          </div>
          <h3 className="text-base-content m-0 mb-1 text-lg leading-tight">
            {p.name}
          </h3>
          <div className="mt-4 flex flex-col gap-2">
            {p.assets.map((a) => (
              <a key={a.href} href={a.href} className="btn btn-primary btn-sm gap-2">
                <FaDownload className="h-3.5 w-3.5" />
                {a.label}
              </a>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

const Downloads: FC = () => (
  <main>
    <section className="py-20">
      <div className="mx-auto w-full max-w-6xl px-5">
        <div className="mx-auto mb-12 max-w-2xl text-center">
          <h1 className="m-0 text-4xl leading-tight tracking-normal text-balance">
            Download Monkey
          </h1>
          <p className="mt-3 text-lg text-gray-500">
            The Monkey C toolchain for Garmin Connect IQ — CLI binaries and
            editor extensions. Assets are in the{' '}
            <a href={RELEASE_PAGE} className="text-primary">
              latest release
            </a>
            .
          </p>
        </div>

        <h2 className="text-base-content mb-6 flex items-center gap-2 text-2xl leading-tight">
          <FaTerminal className="text-primary h-6 w-6" />
          CLI
        </h2>
        <div className="mb-12">
          <PlatformGrid platforms={CLI} />
        </div>

        <h2 className="text-base-content mb-6 flex items-center gap-2 text-2xl leading-tight">
          <FaPuzzlePiece className="text-primary h-6 w-6" />
          Editor extensions
        </h2>
        <div className="border-base-300 bg-base-200/50 mb-12 rounded-2xl border p-6">
          <div className="flex flex-col gap-2 sm:flex-row">
            {EXTENSIONS.map((a) => (
              <a key={a.href} href={a.href} className="btn btn-primary btn-sm gap-2">
                <FaDownload className="h-3.5 w-3.5" />
                {a.label}
              </a>
            ))}
          </div>
        </div>

        <p className="text-base-content/60 text-center text-sm">
          Or build the CLI from source:{' '}
          <code className="bg-base-300 rounded px-1.5 py-0.5 font-mono text-xs">
            cargo build --release -p monkey-cli
          </code>{' '}
          in the{' '}
          <a href={GITHUB} className="text-primary">
            repository
          </a>
          .
        </p>
      </div>
    </section>
  </main>
);

export default Downloads;
