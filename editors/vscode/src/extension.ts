import * as vscode from "vscode";
import {
    LanguageClient,
    ServerOptions,
    TransportKind,
    LanguageClientOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

function getServerPath(): string {
    const config = vscode.workspace.getConfiguration("sqlalchemy-lsp");
    return config.get<string>("serverPath", "sqlalchemy-lsp");
}

export function activate(context: vscode.ExtensionContext) {
    const server: ServerOptions = {
        command: getServerPath(),
        args: ["lsp", "--stdio"],
        transport: TransportKind.stdio,
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: "python" }],
    };

    client = new LanguageClient(
        "sqlalchemy-lsp",
        "SQLAlchemy LSP",
        server,
        clientOptions,
    );

    client.start();

    context.subscriptions.push(
        vscode.commands.registerCommand("sqlalchemy-lsp.showSchema", async () => {
            const result = await client.sendRequest("workspace/executeCommand", {
                command: "sqlalchemy.showSchema",
                arguments: ["mermaid"],
            });
            if (typeof result === "string") {
                const doc = await vscode.workspace.openTextDocument({
                    content: result,
                    language: "markdown",
                });
                vscode.window.showTextDocument(doc);
            }
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
