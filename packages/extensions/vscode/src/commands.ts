import * as path from 'path';
import * as cp from 'child_process';
import * as vscode from 'vscode';
import { findBuildable } from './runtime-preview';

interface MonkeyCommand {
  id: string;
  title: string;
  args: string[];
}

const COMMANDS: MonkeyCommand[] = [
  { id: 'monkeyc.build', title: 'Build PRG', args: ['build'] },
  { id: 'monkeyc.run', title: 'Build and Run (Simulator)', args: ['run'] },
  { id: 'monkeyc.test', title: 'Test', args: ['test'] },
  { id: 'monkeyc.export', title: 'Export .iq', args: ['export'] },
  { id: 'monkeyc.debug', title: 'Debug (Monkeybrains)', args: ['debug'] },
  { id: 'monkeyc.clean', title: 'Clean Build Output', args: ['clean'] },
  { id: 'monkeyc.projectInfo', title: 'Project Info', args: ['project', 'info'] },
  { id: 'monkeyc.doctor', title: 'Doctor (SDK/Key Check)', args: ['doctor'] },
];

let output: vscode.OutputChannel | null = null;

function channel(): vscode.OutputChannel {
  if (!output) output = vscode.window.createOutputChannel('Monkey');
  return output;
}

function cliPath(): string {
  return vscode.workspace.getConfiguration('monkeyC').get<string>('cliPath') || 'monkey';
}

/// Nearest directory containing monkey.jungle + manifest.xml from the active
/// file, else the first workspace folder.
function projectDir(): string | null {
  const editor = vscode.window.activeTextEditor;
  const start =
    editor && editor.document.uri.scheme === 'file'
      ? path.dirname(editor.document.uri.fsPath)
      : vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  if (!start) return null;
  return findBuildable(start)?.dir ?? start;
}

function run(args: string[], cwd: string): Promise<number> {
  const cmd = cliPath();
  const out = channel();
  out.appendLine(`$ ${[cmd, ...args].join(' ')}`);
  return new Promise((resolve, reject) => {
    const child = cp.spawn(cmd, args, { cwd, env: process.env });
    child.stdout.on('data', (d: Buffer | string) => out.append(d.toString()));
    child.stderr.on('data', (d: Buffer | string) => out.append(d.toString()));
    child.on('error', reject);
    child.on('close', (code) => resolve(code ?? 1));
  });
}

async function execMonkey(spec: MonkeyCommand): Promise<void> {
  const dir = projectDir();
  if (!dir) {
    vscode.window.showErrorMessage(
      'No workspace folder or Connect IQ package found (needs monkey.jungle and manifest.xml).',
    );
    return;
  }
  const out = channel();
  out.show(true);
  try {
    const code = await vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: `Monkey C: ${spec.title}`,
        cancellable: false,
      },
      () => run([...spec.args, dir], dir),
    );
    if (code !== 0) {
      vscode.window.showErrorMessage(
        `Monkey C: ${spec.title} failed (exit ${code}) — see the Monkey output channel.`,
      );
    }
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    out.appendLine(e instanceof Error ? e.stack || e.message : String(e));
    vscode.window.showErrorMessage(
      `Monkey C: ${spec.title} failed: ${message}. Is the 'monkey' CLI on PATH?`,
    );
  }
}

export function registerMonkeyCommands(context: vscode.ExtensionContext): void {
  for (const spec of COMMANDS) {
    context.subscriptions.push(vscode.commands.registerCommand(spec.id, () => execMonkey(spec)));
  }
}
