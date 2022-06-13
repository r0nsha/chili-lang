/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */
import * as fs from "fs";
import * as tmp from "tmp";
import * as path from "path";
import * as util from "node:util";

import {
  createConnection,
  TextDocuments,
  Diagnostic,
  DiagnosticSeverity,
  ProposedFeatures,
  InitializeParams,
  DidChangeConfigurationNotification,
  CompletionItem,
  CompletionItemKind,
  TextDocumentPositionParams,
  TextDocumentSyncKind,
  InitializeResult,
} from "vscode-languageserver/node";

import { TextDocument } from "vscode-languageserver-textdocument";

interface ChiliTextDocument extends TextDocument {
  chiliInlayHints?: InlayHint[];
}

import {
  Position,
  InlayHint,
  InlayHintParams,
  InlayHintLabelPart,
  InlayHintKind,
} from "vscode-languageserver-protocol";

// Create a connection for the server, using Node's IPC as a transport.
// Also include all preview / proposed LSP features.
const connection = createConnection(ProposedFeatures.all);

// Create a simple text document manager.
const documents: TextDocuments<TextDocument> = new TextDocuments(TextDocument);

let hasConfigurationCapability = false;
let hasWorkspaceFolderCapability = false;
let hasDiagnosticRelatedInformationCapability = false;

import { TextEncoder } from "node:util";
// eslint-disable-next-line @typescript-eslint/no-var-requires
const exec = util.promisify(require("node:child_process").exec);

const tmpFile = tmp.fileSync();

// The example settings
interface ExampleSettings {
  maxNumberOfProblems: number;
}

// The global settings, used when the `workspace/configuration` request is not supported by the client.
// Please note that this is not the case when using this server with the client provided in this example
// but could happen with other clients.
const defaultSettings: ExampleSettings = { maxNumberOfProblems: 1000 };
let globalSettings: ExampleSettings = defaultSettings;

// Cache the settings of all open documents
const documentSettings: Map<string, Thenable<ExampleSettings>> = new Map();

connection.onInitialize(({ capabilities }: InitializeParams) => {
  // Does the client support the `workspace/configuration` request?
  // If not, we fall back using global settings.
  hasConfigurationCapability = !!(
    capabilities.workspace && !!capabilities.workspace.configuration
  );

  hasWorkspaceFolderCapability = !!(
    capabilities.workspace && !!capabilities.workspace.workspaceFolders
  );

  hasDiagnosticRelatedInformationCapability = !!(
    capabilities.textDocument &&
    capabilities.textDocument.publishDiagnostics &&
    capabilities.textDocument.publishDiagnostics.relatedInformation
  );

  const result: InitializeResult = {
    capabilities: {
      textDocumentSync: TextDocumentSyncKind.Full,
    },
  };

  if (hasWorkspaceFolderCapability) {
    result.capabilities.workspace = {
      workspaceFolders: {
        supported: true,
      },
    };
  }

  console.log("Chili language server initialized");

  return result;
});

connection.onInitialized(() => {
  if (hasConfigurationCapability) {
    // Register for all configuration changes.
    connection.client.register(
      DidChangeConfigurationNotification.type,
      undefined
    );
  }
  if (hasWorkspaceFolderCapability) {
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    connection.workspace.onDidChangeWorkspaceFolders((_event) => {
      connection.console.log("Workspace folder change event received.");
    });
  }
});

connection.onDidChangeConfiguration((change) => {
  if (hasConfigurationCapability) {
    // Reset all cached document settings
    documentSettings.clear();
  } else {
    globalSettings = <ExampleSettings>(
      (change.settings.languageServerExample || defaultSettings)
    );
  }

  // Revalidate all open text documents
  documents.all().forEach(validateTextDocument);
});

documents.onDidChangeContent(
  (() => {
    const throttledValidateTextDocument = throttle(validateTextDocument, 500);
    return (change) => {
      throttledValidateTextDocument(change.document);
    };
  })()
);

// eslint-disable-next-line @typescript-eslint/no-unused-vars
function getDocumentSettings(resource: string): Thenable<ExampleSettings> {
  if (!hasConfigurationCapability) {
    return Promise.resolve(globalSettings);
  }
  let result = documentSettings.get(resource);
  if (!result) {
    result = connection.workspace.getConfiguration({
      scopeUri: resource,
      section: "chiliLanguageServer",
    });
    documentSettings.set(resource, result);
  }
  return result;
}

// Only keep settings for open documents
documents.onDidClose((e) => {
  documentSettings.delete(e.document.uri);
});

async function validateTextDocument(
  textDocument: ChiliTextDocument
): Promise<void> {
  console.time("validateTextDocument");

  if (!hasDiagnosticRelatedInformationCapability) {
    console.error(
      "Trying to validate a document with no diagnostic capability"
    );
    return;
  }

  // The validator creates diagnostics for all uppercase words length 2 and more
  const text = textDocument.getText();

  // const lineBreaks = findLineBreaks(text);

  textDocument.chiliInlayHints = [];

  const diagnostics: Diagnostic[] = [];

  // const seenTypeHintPositions = new Set();

  const stdout = await runCompiler(text, includeFlagForPath(textDocument.uri));
  // const stdout = await runCompiler2(textDocument.uri, "");

  const lines = stdout.split("\n").filter((l) => l.length > 0);

  for (const line of lines) {
    // console.log(line);
    try {
      const objects: LspObject[] = JSON.parse(line);
      // console.log(objects);

      for (const object of objects) {
        if (object.type == "diagnostic") {
          const diagnostic = object.diagnostic;

          let severity = DiagnosticSeverity.Error;

          switch (diagnostic.severity) {
            case LspDiagnosticSeverity.Error:
              severity = DiagnosticSeverity.Error;
              break;
          }

          const sourceUri = "file://" + diagnostic.source;
          const sourceDocument =
            diagnostic.source == tmpFile.name
              ? textDocument
              : documents.get(sourceUri);

          if (!sourceDocument) {
            console.error(`couldn't open text document: ${sourceUri}`);
            continue;
          }

          const start = sourceDocument.positionAt(diagnostic.span.start);
          const end = sourceDocument.positionAt(diagnostic.span.end);

          diagnostics.push({
            severity,
            range: { start, end },
            message: diagnostic.message,
            source: sourceDocument.uri.toString(),
          });
        }
        //     } else if (obj.type == "hint") {
        //       if (!seenTypeHintPositions.has(obj.position)) {
        //         seenTypeHintPositions.add(obj.position);
        //         const position = convertSpan(obj.position, lineBreaks);
        //         const hint_string = ": " + obj.typename;
        //         const hint = InlayHint.create(
        //           position,
        //           [InlayHintLabelPart.create(hint_string)],
        //           InlayHintKind.Type
        //         );
        //         textDocument.chiliInlayHints.push(hint);
        //       }
        //     }
      }
    } catch (e) {
      console.error(e);
    }
  }

  // Send the computed diagnostics to VSCode.
  connection.sendDiagnostics({ uri: textDocument.uri, diagnostics });

  console.timeEnd("validateTextDocument");
}

function lowerBoundBinarySearch(arr: number[], num: number): number {
  let low = 0;
  let mid = 0;
  let high = arr.length - 1;

  if (num >= arr[high]) return high;

  while (low < high) {
    // Bitshift to avoid floating point division
    mid = (low + high) >> 1;

    if (arr[mid] < num) {
      low = mid + 1;
    } else {
      high = mid;
    }
  }

  return low - 1;
}

function convertSpan(utf8_offset: number, lineBreaks: Array<number>): Position {
  const lineBreakIndex = lowerBoundBinarySearch(lineBreaks, utf8_offset);

  const start_of_line_offset =
    lineBreakIndex == -1 ? 0 : lineBreaks[lineBreakIndex] + 1;
  const character = utf8_offset - start_of_line_offset;

  return { line: lineBreakIndex + 1, character };
}

function convertPosition(position: Position, text: string): number {
  let line = 0;
  let character = 0;
  const buffer = new TextEncoder().encode(text);

  let i = 0;
  while (i < text.length) {
    if (line == position.line && character == position.character) {
      return i;
    }

    if (buffer.at(i) == 0x0a) {
      line++;
      character = 0;
    } else {
      character++;
    }

    i++;
  }

  return i;
}

function findLineBreaks(utf16_text: string): Array<number> {
  const utf8_text = new TextEncoder().encode(utf16_text);
  const lineBreaks: Array<number> = [];

  for (let i = 0; i < utf8_text.length; ++i) {
    if (utf8_text[i] == 0x0a) {
      lineBreaks.push(i);
    }
  }

  return lineBreaks;
}

async function runCompiler(text: string, flags: string): Promise<string> {
  try {
    fs.writeFileSync(tmpFile.name, text);
  } catch (error) {
    console.log(error);
  }

  let stdout = "";

  try {
    const output = await exec(`chili check ${tmpFile.name} ${flags}`);
    // console.log(output);
    if (output.stderr != null && output.stderr != "") {
      console.error(output.stderr);
    } else {
      stdout = output.stdout;
    }
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
  } catch (e: any) {
    console.log(e);
    stdout = e.stdout ?? "";
  }

  return stdout;
}

function includeFlagForPath(file_path: string): string {
  const protocol_end = file_path.indexOf("://");
  if (protocol_end == -1) return " --include-paths " + file_path;
  // Not protocol.length + 3, include the last '/'
  return " --include-paths " + path.dirname(file_path.slice(protocol_end + 2));
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function throttle(fn: (...args: any) => void, delay: number) {
  let shouldWait = false;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let waitingArgs: any | null;
  const timeoutFunc = () => {
    if (waitingArgs == null) {
      shouldWait = false;
    } else {
      fn(...waitingArgs);
      waitingArgs = null;
      setTimeout(timeoutFunc, delay);
    }
  };

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (...args: any) => {
    if (shouldWait) {
      waitingArgs = args;
      return;
    }

    fn(...args);
    shouldWait = true;

    setTimeout(timeoutFunc, delay);
  };
}

// Make the text document manager listen on the connection
// for open, change and close text document events
documents.listen(connection);

// Listen on the connection
connection.listen();

export interface Span {
  start: number;
  end: number;
}

export interface LspDiagnostic {
  severity: LspDiagnosticSeverity;
  span: Span;
  message: string;
  source: string;
}

export enum LspDiagnosticSeverity {
  Error,
}

export type LspObject = {
  type: "diagnostic";
  diagnostic: LspDiagnostic;
};
