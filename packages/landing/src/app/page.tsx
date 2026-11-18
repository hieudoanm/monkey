import CTA from '@/components/molecules/CTA';
import Features, { type Feature } from '@/components/molecules/Features';
import Hero from '@/components/molecules/Hero';
import SectionHead from '@/components/molecules/SectionHead';
import Terminal from '@/components/molecules/Terminal';
import type { Metadata, NextPage } from 'next';
import type { FC } from 'react';
import {
  LuBraces,
  LuCode,
  LuFileCheck,
  LuGlobe,
  LuMonitor,
  LuPackage,
  LuTerminal,
} from 'react-icons/lu';

export const metadata: Metadata = {
  alternates: { canonical: '/' },
};

const GITHUB = 'https://github.com/hieudoanm/monkey';
const VSIX = `${GITHUB}/releases/download/latest/extension-monkey-c.vsix`;
const CLI_LATEST = `${GITHUB}/releases/download/latest`;

const FEATURES: Feature[] = [
  {
    icon: LuTerminal,
    title: 'monkey CLI',
    body: 'Zero-dependency Rust binary with format, lint, check, parse, tokens, doctor, sdk, device, project, build, and run commands — it even compiles PRGs and launches the simulator via the official SDK. Prebuilt for Linux, macOS, and Windows.',
  },
  {
    icon: LuBraces,
    title: 'Parser & linter',
    body: 'A real Monkey C parser powers lint rules — unused variables, unreachable code, identical operands, empty blocks — plus lexer/parser diagnostics. JSON and SARIF output for CI.',
  },
  {
    icon: LuFileCheck,
    title: 'monkey check',
    body: 'One command runs lint plus a format-drift check, designed for pre-commit hooks and CI. Exits non-zero on any finding.',
  },
  {
    icon: LuCode,
    title: 'VS Code extension',
    body: 'Syntax highlighting, document formatting, inline lint diagnostics via monkey lint --stdin, a dark theme, file icons, and runtime preview — packaged as a .vsix.',
  },
  {
    icon: LuGlobe,
    title: 'Zed extension',
    body: 'Loaded as a dev extension from source: Tree-sitter highlighting, external formatter, lint/check tasks, runtime preview, and the Monkey C theme and icons.',
  },
  {
    icon: LuMonitor,
    title: 'Build & run',
    body: 'monkey build compiles a PRG with the official monkeyc; monkey run builds, starts the simulator, and deploys with monkeydo — the same flow as the editor runtime preview, from the terminal.',
  },
  {
    icon: LuPackage,
    title: 'SDK & device discovery',
    body: 'monkey doctor, sdk, and device commands find your Connect IQ SDK, developer key, and 180+ device profiles using the same lookup order as the editor extensions — never hardcoded.',
  },
];

const HERO_LINES = [
  '$ monkey check .',
  'checked 12 files — no issues',
  '$ monkey format --write source/App.mc',
  'formatted source/App.mc',
];

const DEMO_TERMINALS: { caption: string; lines: string[]; wide?: boolean }[] = [
  {
    caption: 'Lint and format-check a project',
    wide: true,
    lines: ['$ monkey check .', 'checked 12 files — no issues'],
  },
  {
    caption: 'Verify SDK, key, and project setup',
    lines: [
      '$ monkey doctor .',
      'ok   sdk        …/Sdks/connectiq-sdk-mac-9.3.0 (9.3.0) — 3/3 tools',
      'ok   key        ~/.config/garmin/developer_key',
      'ok   devices    180 device profile(s)',
      'ok   project    MyApp (watchface) → fr55',
      'doctor: ready',
    ],
  },
  {
    caption: 'Format a Monkey C file',
    lines: ['$ monkey format --write source/App.mc', 'formatted source/App.mc'],
  },
  {
    caption: 'Editor integration via stdin',
    lines: [
      '$ monkey format --stdin < App.mc',
      'class App extends Application {',
      '    function initialize() {',
      '    }',
      '}',
    ],
  },
  {
    caption: 'Parse check',
    lines: ['$ monkey parse App.mc', 'App.mc: ok'],
  },
  {
    caption: 'Build a PRG with the official SDK',
    wide: true,
    lines: [
      '$ monkey build apps/hands',
      'monkey: building … (watchface) → fr55 (fr55) with SDK …/connectiq-sdk-mac-9.3.0',
      'BUILD SUCCESSFUL',
      'apps/hands/bin/hands.prg',
    ],
  },
];

const STEPS = [
  {
    title: 'Install the CLI',
    body: 'Download the cli-monkey-* binary for your platform from the latest release, or build from source with cargo build --release -p monkey-cli.',
    snippet: 'cli-monkey-x86_64-unknown-linux-gnu → PATH',
  },
  {
    title: 'Wire up your editor',
    body: 'Install the VS Code .vsix, or load packages/extensions/zed as a dev extension in Zed. Formatters share the same rules as the CLI.',
    snippet: 'code --install-extension extension-monkey-c.vsix',
  },
  {
    title: 'Preview in the simulator',
    body: 'Point the extension at your Connect IQ SDK and developer key, then run Monkey C: Runtime Preview or Build PRG.',
    snippet: 'task: Monkey C: Runtime Preview',
  },
];

const INSTALL_LINES = [
  '# CLI binaries (Linux x86_64, macOS arm64, Windows x86_64)',
  `$ curl -fsSLO ${CLI_LATEST}/cli-monkey-x86_64-unknown-linux-gnu`,
  '$ chmod +x cli-monkey-x86_64-unknown-linux-gnu && mv cli-monkey-x86_64-unknown-linux-gnu ~/.local/bin/monkey',
  '',
  '# VS Code extension',
  '$ curl -fsSLO https://github.com/hieudoanm/monkey/releases/latest/download/extension-monkey-c.vsix',
  '$ code --install-extension extension-monkey-c.vsix',
];

const StepCard: FC<{ step: (typeof STEPS)[number]; index: number }> = ({
  step,
  index,
}) => (
  <article className="card border-base-300 bg-base-100 rounded-2xl border p-6">
    <span className="text-accent mb-2 block font-mono text-sm font-semibold">
      {String(index + 1).padStart(2, '0')}
    </span>
    <h3 className="text-base-content m-0 mb-2 text-lg font-bold">
      {step.title}
    </h3>
    <p className="text-base-content/60 text-sm">{step.body}</p>
    <code className="border-base-300 bg-base-200 text-base-content/60 mt-4 block overflow-x-auto rounded-lg border px-3 py-0.5 font-mono text-xs">
      {step.snippet}
    </code>
  </article>
);

const HomePage: NextPage = () => (
  <main>
    <Hero
      title="The monkey toolchain for Connect IQ."
      description="A zero-dependency Rust CLI that formats, lints, and checks Monkey C and Jungle files — plus VS Code and Zed extensions with highlighting, formatting, themes, and simulator preview."
      primary={{ label: 'Get the CLI', href: '#install' }}
      secondary={{ label: 'See it in action', href: '#demo' }}
      stats={[
        { value: '11', label: 'CLI commands' },
        { value: '2', label: 'editors supported' },
        { value: '0', label: 'runtime dependencies' },
      ]}>
      <Terminal lines={HERO_LINES} />
    </Hero>

    <Features
      title="Everything for Monkey C"
      description="One toolchain shared by the CLI, VS Code, and Zed — same formatter, same rules, same preview flow."
      features={FEATURES}
    />

    <section id="cli" className="bg-base-200 py-20">
      <div className="mx-auto w-full max-w-6xl px-5">
        <SectionHead
          title="The monkey CLI"
          description="format, lint, check, parse, doctor, sdk, device, project, build, and run — files or whole directories, with editor-friendly stdin/stdout, JSON/SARIF, and SDK build modes."
        />
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {DEMO_TERMINALS.map((t) => (
            <Terminal
              key={t.caption}
              lines={t.lines}
              className={t.wide ? 'lg:col-span-2' : ''}
            />
          ))}
        </div>
        <p className="text-base-content/60 mt-6 text-center text-sm">
          Build from source:{' '}
          <code className="border-base-300 bg-base-100 rounded border px-1.5 py-0.5 font-mono text-xs">
            cd crates && cargo build --release -p monkey-cli
          </code>
        </p>
      </div>
    </section>

    <section id="extensions" className="py-20">
      <div className="mx-auto w-full max-w-6xl px-5">
        <SectionHead
          title="Editor extensions"
          description="VS Code ships as a .vsix; Zed loads from source. Both share the formatter rules and the runtime-preview flow."
        />
        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          <article className="card border-base-300 bg-base-100 rounded-2xl border p-6">
            <LuPackage className="text-primary mb-3 h-6 w-6" />
            <h3 className="text-base-content m-0 mb-2 text-lg font-bold">
              VS Code — monkey-c
            </h3>
            <p className="text-base-content/60 text-sm">
              Highlighting for .mc/.mb/.mcgen/.jungle, document formatting,
              inline lint diagnostics, dark theme, file icons, breakpoints, and
              Monkey C: Runtime Preview (Simulator).
            </p>
            <a
              href={VSIX}
              target="_blank"
              rel="noreferrer"
              className="text-primary mt-4 inline-block text-sm font-semibold">
              Download VSIX →
            </a>
          </article>
          <article className="card border-base-300 bg-base-100 rounded-2xl border p-6">
            <LuGlobe className="text-primary mb-3 h-6 w-6" />
            <h3 className="text-base-content m-0 mb-2 text-lg font-bold">
              Zed — Monkey C
            </h3>
            <p className="text-base-content/60 text-sm">
              Tree-sitter languages with highlighting, brackets, folds, outline,
              and text objects; external formatter; lint/check tasks; theme and
              icons; Runtime Preview / Build PRG tasks.
            </p>
            <a
              href={`${GITHUB}/tree/master/packages/extensions/zed`}
              target="_blank"
              rel="noreferrer"
              className="text-primary mt-4 inline-block text-sm font-semibold">
              packages/extensions/zed →
            </a>
          </article>
        </div>
      </div>
    </section>

    <section id="how-it-works" className="bg-base-200 py-20">
      <div className="mx-auto w-full max-w-6xl px-5">
        <SectionHead
          title="How it works"
          description="From download to simulator in three steps."
        />
        <div className="grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-3">
          {STEPS.map((step, i) => (
            <StepCard key={step.title} step={step} index={i} />
          ))}
        </div>
      </div>
    </section>

    <section id="install" className="py-20">
      <div className="mx-auto w-full max-w-3xl px-5">
        <SectionHead
          title="Get the latest release"
          description="Rolling latest release built from this repository — CLI binaries, the VS Code .vsix, and the Zed extension zip."
        />
        <Terminal lines={INSTALL_LINES} />
        <p className="text-base-content/60 mt-6 text-center text-sm">
          For Zed: open the Extensions page, choose{' '}
          <em>Install Dev Extension</em>, and select packages/extensions/zed —{' '}
          or use the{' '}
          <code className="font-mono text-xs">extension-monkey-c-zed.zip</code>{' '}
          asset as a source snapshot. See the{' '}
          <a
            href={`${GITHUB}#readme`}
            target="_blank"
            rel="noreferrer"
            className="text-primary font-semibold">
            README
          </a>{' '}
          for the full setup.
        </p>
      </div>
    </section>

    <CTA
      title="Ready to write Monkey C?"
      description="Install the CLI, wire up your editor, and preview on the simulator."
      primary={{ label: 'Download VSIX', href: VSIX, external: true }}
      secondary={{ label: 'View on GitHub', href: GITHUB, external: true }}
    />
  </main>
);

export default HomePage;
