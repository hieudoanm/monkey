import * as vscode from 'vscode';
import { LanguageClient, TransportKind } from 'vscode-languageclient/node';

/**
 * `monkey lsp` language client — provides diagnostics plus hover,
 * definition, completion, formatting, references, rename, and signature
 * help for Monkey C and Jungle documents.
 *
 * Returns the started client, or `null` when the CLI can't be launched
 * (caller should fall back to the lint provider).
 */
export async function startLanguageClient(
  context: vscode.ExtensionContext,
): Promise<LanguageClient | null> {
  const cliPath = vscode.workspace.getConfiguration('monkeyC').get<string>('cliPath', 'monkey');

  const client = new LanguageClient(
    'monkeyc',
    'Monkey C Language Server',
    {
      command: cliPath,
      args: ['lsp'],
      transport: TransportKind.stdio,
    },
    {
      documentSelector: [
        { scheme: 'file', language: 'monkeyc' },
        { scheme: 'file', language: 'jungle' },
        { scheme: 'untitled', language: 'monkeyc' },
        { scheme: 'untitled', language: 'jungle' },
      ],
      synchronize: {
        configurationSection: 'monkeyC',
      },
    },
  );

  try {
    await client.start();
  } catch {
    return null;
  }
  context.subscriptions.push(client);
  return client;
}
