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

export class MonkeyLintProvider implements vscode.Disposable {
  private readonly collection = vscode.languages.createDiagnosticCollection(COLLECTION);
  private readonly timers = new Map<string, NodeJS.Timeout>();
  private readonly running = new Map<string, ChildProcess>();
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
    this.collection.dispose();
    for (const s of this.subscriptions) s.dispose();
  }

  private enabled(): boolean {
    return vscode.workspace.getConfiguration('monkeyC').get<boolean>('lintEnabled', true);
  }

  private cliPath(): string {
    return vscode.workspace.getConfiguration('monkeyC').get<string>('cliPath', 'monkey');
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
    this.running.get(key)?.kill();
    const src = doc.getText();
    // Label determines language detection in the CLI; unsaved buffers get a
    // synthetic extension matching the document language.
    const label = doc.isUntitled
      ? `<stdin>.${doc.languageId === 'jungle' ? 'jungle' : 'mc'}`
      : doc.fileName;
    const version = doc.version;

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
    });
    child.stdin?.on('error', () => {});
    child.stdin?.write(src);
    child.stdin?.end();
  }
}
