import * as vscode from 'vscode';
import { LanguageClient } from 'vscode-languageclient/node';
import { formatJungle, formatMonkeyC, type FormatOptions } from './format-monkeyc';
import { MonkeyLintProvider } from './lint-monkeyc';
import { startLanguageClient } from './lsp-client';
import { runtimePreview } from './runtime-preview';

function fullRange(document: vscode.TextDocument): vscode.Range {
  const last = document.lineAt(Math.max(0, document.lineCount - 1));
  return new vscode.Range(0, 0, last.range.end.line, last.range.end.character);
}

function makeEdit(document: vscode.TextDocument, formatted: string): vscode.TextEdit[] {
  if (formatted === document.getText()) return [];
  return [vscode.TextEdit.replace(fullRange(document), formatted)];
}

function formattingOptions(
  options: vscode.FormattingOptions,
  document: vscode.TextDocument,
): FormatOptions {
  return {
    insertSpaces: options.insertSpaces,
    tabSize: options.tabSize,
    eol: document.eol === vscode.EndOfLine.CRLF ? '\r\n' : '\n',
  };
}

export function activate(context: vscode.ExtensionContext): void {
  // Prefer `monkey lsp` (diagnostics + hover/definition/completion/
  // references/rename/signatureHelp/formatting). When the CLI can't be
  // launched, fall back to the lint-only diagnostic provider.
  let client: LanguageClient | null = null;
  let lintProvider: MonkeyLintProvider | null = null;

  const useLintFallback = vscode.workspace
    .getConfiguration('monkeyC')
    .get<boolean>('lintEnabled', true);

  const ensureClient = async (): Promise<void> => {
    if (vscode.workspace.getConfiguration('monkeyC').get<boolean>('lspEnabled', true) === false) {
      if (useLintFallback && !lintProvider) {
        lintProvider = new MonkeyLintProvider();
        context.subscriptions.push(lintProvider);
      }
      return;
    }
    const started = await startLanguageClient(context);
    if (started) {
      client = started;
      lintProvider?.dispose();
      lintProvider = null;
    } else if (useLintFallback && !lintProvider) {
      lintProvider = new MonkeyLintProvider();
      context.subscriptions.push(lintProvider);
    }
  };

  void ensureClient();

  // Restart the language client when monkeyC.cliPath changes.
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (!e.affectsConfiguration('monkeyC.cliPath')) return;
      const old = client;
      client = null;
      void old?.stop().then(() => ensureClient());
    }),
  );

  const monkeyc = vscode.languages.registerDocumentFormattingEditProvider(
    { language: 'monkeyc', scheme: 'file' },
    {
      provideDocumentFormattingEdits(document, options) {
        const formatted = formatMonkeyC(document.getText(), formattingOptions(options, document));
        return makeEdit(document, formatted);
      },
    },
  );

  const jungle = vscode.languages.registerDocumentFormattingEditProvider(
    { language: 'jungle', scheme: 'file' },
    {
      provideDocumentFormattingEdits(document, options) {
        const formatted = formatJungle(document.getText(), formattingOptions(options, document));
        return makeEdit(document, formatted);
      },
    },
  );

  const formatCommand = vscode.commands.registerCommand('monkeyc.formatDocument', async () => {
    await vscode.commands.executeCommand('editor.action.formatDocument');
  });
  const previewCommand = vscode.commands.registerCommand('monkeyc.runtimePreview', async () => {
    await runtimePreview(context);
  });

  context.subscriptions.push(monkeyc, jungle, formatCommand, previewCommand);
}

export async function deactivate(): Promise<void> {
  // LanguageClient disposes itself via context.subscriptions.
}
