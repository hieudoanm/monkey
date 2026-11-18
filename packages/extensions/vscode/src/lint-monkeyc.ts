import * as vscode from 'vscode';
import { spawn, type ChildProcess } from 'node:child_process';
import { parseFindings, positionAtByte, type MonkeyFinding } from './lint-findings';

const COLLECTION = 'monkeyc';
const DEBOUNCE_MS = 400;

function severityOf(s: string): vscode.DiagnosticSeverity {
  switch (s) {
    case 'error':
      return vscode.DiagnosticSeverity.Error;
    case 'warning':
      return vscode.DiagnosticSeverity.Warning;
    case 'information':
      return vscode.DiagnosticSeverity.Information;
    default:
      return vscode.DiagnosticSeverity.Hint;
  }
}

/** Convert findings (computed against `src`) into VS Code diagnostics. */
export function toDiagnostics(findings: MonkeyFinding[], src: string): vscode.Diagnostic[] {
  return findings.map((f) => {
    const line = Math.max(0, (f.line ?? 1) - 1);
    const col = Math.max(0, (f.column ?? 1) - 1);
    const start = f.span ? positionAtByte(src, f.span.start) : { line, character: col };
    const end = f.span
      ? positionAtByte(src, f.span.end)
      : { line: start.line, character: start.character + 1 };
    const d = new vscode.Diagnostic(
      new vscode.Range(start.line, start.character, end.line, end.character),
      f.message,
      severityOf(f.severity),
    );
    d.code = f.code;
    d.source = 'monkey';
    return d;
  });
}

/**
 * Persistent `monkey lint --serve` process. Speaks the CLI's frame protocol:
 * `<label>\n<byteLen>\n<source>` in, one JSON diagnostics array line out.
 * Falls back to per-request `lint --stdin` spawns when unavailable.
 */
class LintServer {
  private child: ChildProcess | null = null;
  private buf = '';
  private queue: Array<(line: string | null) => void> = [];
  private dead = false;

  constructor(private readonly cliPath: string) {}

  private start(): boolean {
    if (this.child && !this.dead) return true;
    try {
      const child = spawn(this.cliPath, ['lint', '--serve'], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });
      this.child = child;
      this.dead = false;
      this.buf = '';
      this.queue = [];
      child.stdout?.on('data', (d: Buffer) => this.onData(d.toString()));
      child.on('error', () => this.failAll());
      child.on('close', () => {
        this.child = null;
        this.failAll();
      });
      child.stderr?.on('data', () => {});
      return true;
    } catch {
      return false;
    }
  }

  private onData(chunk: string): void {
    this.buf += chunk;
    let idx: number;
    while ((idx = this.buf.indexOf('\n')) >= 0) {
      const line = this.buf.slice(0, idx);
      this.buf = this.buf.slice(idx + 1);
      const resolve = this.queue.shift();
      resolve?.(line);
    }
  }

  private failAll(): void {
    this.dead = true;
    for (const r of this.queue.splice(0)) r(null);
  }

  /** Lint `src` labelled `label`; resolves to the raw JSON line or null. */
  request(label: string, src: string): Promise<string | null> {
    if (!this.start() || !this.child?.stdin) return Promise.resolve(null);
    const child = this.child;
    return new Promise((resolve) => {
      this.queue.push(resolve);
      child.stdin!.write(`${label}\n${Buffer.byteLength(src, 'utf8')}\n${src}`);
    });
  }

  dispose(): void {
    this.failAll();
    this.child?.kill();
    this.child = null;
  }
}

export class MonkeyLintProvider implements vscode.Disposable {
  private readonly collection = vscode.languages.createDiagnosticCollection(COLLECTION);
  private readonly timers = new Map<string, NodeJS.Timeout>();
  private readonly running = new Map<string, ChildProcess>();
  private server: LintServer | null = null;
  private serverPath = '';
  private readonly subscriptions: vscode.Disposable[] = [];

  constructor() {
    this.subscriptions.push(
      vscode.workspace.onDidOpenTextDocument((d) => this.schedule(d)),
      vscode.workspace.onDidChangeTextDocument((e) => this.schedule(e.document)),
      vscode.workspace.onDidSaveTextDocument((d) => this.runNow(d)),
      vscode.workspace.onDidCloseTextDocument((d) => this.clear(d.uri)),
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('monkeyC')) this.refreshAll();
      }),
    );
    for (const doc of vscode.workspace.textDocuments) this.schedule(doc);
  }

  dispose(): void {
    for (const t of this.timers.values()) clearTimeout(t);
    for (const p of this.running.values()) p.kill();
    this.server?.dispose();
    this.collection.dispose();
    for (const s of this.subscriptions) s.dispose();
  }

  private enabled(): boolean {
    return vscode.workspace.getConfiguration('monkeyC').get<boolean>('lintEnabled', true);
  }

  private cliPath(): string {
    return vscode.workspace.getConfiguration('monkeyC').get<string>('cliPath', 'monkey');
  }

  /** Persistent lint server for the current cliPath (restarted on change). */
  private lintServer(): LintServer | null {
    const p = this.cliPath();
    if (this.server && this.serverPath === p) return this.server;
    this.server?.dispose();
    this.server = new LintServer(p);
    this.serverPath = p;
    return this.server;
  }

  private isLintable(doc: vscode.TextDocument): boolean {
    return (
      this.enabled() &&
      (doc.languageId === 'monkeyc' || doc.languageId === 'jungle') &&
      doc.getText().length > 0
    );
  }

  private schedule(doc: vscode.TextDocument): void {
    if (!this.isLintable(doc)) return;
    const key = doc.uri.toString();
    const existing = this.timers.get(key);
    if (existing) clearTimeout(existing);
    this.timers.set(
      key,
      setTimeout(() => this.runNow(doc), DEBOUNCE_MS),
    );
  }

  private clear(uri: vscode.Uri): void {
    this.collection.delete(uri);
    const key = uri.toString();
    const t = this.timers.get(key);
    if (t) clearTimeout(t);
    this.timers.delete(key);
    this.running.get(key)?.kill();
    this.running.delete(key);
  }

  private refreshAll(): void {
    const uris: vscode.Uri[] = [];
    this.collection.forEach((uri) => uris.push(uri));

    for (const uri of uris) this.collection.delete(uri);
    for (const doc of vscode.workspace.textDocuments) this.schedule(doc);
  }

  private runNow(doc: vscode.TextDocument): void {
    if (!this.isLintable(doc)) {
      this.collection.delete(doc.uri);
      return;
    }
    const key = doc.uri.toString();
    const src = doc.getText();
    // Label determines language detection in the CLI; unsaved buffers get a
    // synthetic extension matching the document language.
    const label = doc.isUntitled
      ? `<stdin>.${doc.languageId === 'jungle' ? 'jungle' : 'mc'}`
      : doc.fileName;
    const version = doc.version;

    // Preferred path: persistent `lint --serve` process (one spawn total).
    const server = this.lintServer();
    if (server) {
      void server.request(label, src).then((line) => {
        if (line === null) {
          this.spawnLint(doc, key, label, src, version);
          return;
        }
        this.applyResult(doc, key, src, version, line, '');
      });
      return;
    }
    this.spawnLint(doc, key, label, src, version);
  }

  /** Fallback: one-shot `monkey lint --stdin` spawn per request. */
  private spawnLint(
    doc: vscode.TextDocument,
    key: string,
    label: string,
    src: string,
    version: number,
  ): void {
    this.running.get(key)?.kill();
    let child: ChildProcess;
    try {
      child = spawn(this.cliPath(), ['lint', '--stdin', label], {
        stdio: ['pipe', 'pipe', 'pipe'],
      });
    } catch {
      return; // CLI not installed; stay silent
    }
    this.running.set(key, child);

    let out = '';
    let err = '';
    child.stdout?.on('data', (d: Buffer) => (out += d.toString()));
    child.stderr?.on('data', (d: Buffer) => (err += d.toString()));
    child.on('error', (e) => {
      this.running.delete(key);
      const d = new vscode.Diagnostic(
        new vscode.Range(0, 0, 0, 1),
        `monkey lint unavailable: ${e.message}`,
        vscode.DiagnosticSeverity.Warning,
      );
      d.source = 'monkey';
      this.collection.set(doc.uri, [d]);
    });
    child.on('close', () => {
      this.running.delete(key);
      this.applyResult(doc, key, src, version, out, err);
    });
    child.stdin?.on('error', () => {});
    child.stdin?.write(src);
    child.stdin?.end();
  }

  /** Convert a lint JSON response into diagnostics for `doc`. */
  private applyResult(
    doc: vscode.TextDocument,
    key: string,
    src: string,
    version: number,
    out: string,
    err: string,
  ): void {
    // Ignore results for stale document versions; a new run is scheduled.
    const current = vscode.workspace.textDocuments.find((d) => d.uri.toString() === key);
    if (!current || current.version !== version) return;
    const findings = parseFindings(out);
    if (findings.length === 0 && err.trim() !== '' && !out.trim().startsWith('[')) {
      // CLI missing or crashed — show a hint once.
      const d = new vscode.Diagnostic(
        new vscode.Range(0, 0, 0, 1),
        `monkey lint unavailable: ${err.trim().split('\n')[0]}`,
        vscode.DiagnosticSeverity.Warning,
      );
      d.source = 'monkey';
      this.collection.set(doc.uri, [d]);
      return;
    }
    this.collection.set(doc.uri, toDiagnostics(findings, src));
  }
}
