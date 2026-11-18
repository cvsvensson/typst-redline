import * as vscode from "vscode";
import * as path from "path";
import { execFile } from "child_process";

function run(cmd: string, args: string[], cwd: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { cwd, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (err) {
        reject(new Error(stderr.trim() || err.message));
      } else {
        resolve(stdout);
      }
    });
  });
}

async function showChanges() {
  const doc = vscode.window.activeTextEditor?.document;
  if (!doc || !doc.fileName.endsWith(".typ")) {
    vscode.window.showErrorMessage("Open a .typ file first.");
    return;
  }
  const file = doc.fileName;
  const dir = path.dirname(file);
  const log = await run("git", ["log", "-n", "40", "--date=short", "--format=%h%x09%ad%x09%s", "--", path.basename(file)], dir);
  const items = log
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => {
      const [hash, date, subject] = l.split("\t");
      return { label: subject, description: `${hash}  ${date}`, rev: hash };
    });
  if (items.length === 0) {
    vscode.window.showErrorMessage("No git history for this file.");
    return;
  }
  const pick = await vscode.window.showQuickPick(items, { placeHolder: "Show changes since commit", matchOnDescription: true });
  if (!pick) {
    return;
  }
  if (doc.isDirty) {
    await doc.save();
  }
  const config = vscode.workspace.getConfiguration("typstRedline");
  const root = config.get<string>("root") || vscode.workspace.getWorkspaceFolder(doc.uri)?.uri.fsPath || dir;
  const exe = config.get<string>("executable") || "typst-redline";
  const pdf = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: `Typst Redline: changes since ${pick.rev}` },
    () => run(exe, [file, "--since", pick.rev, "--root", root], dir),
  );
  const uri = vscode.Uri.file(pdf.trim());
  if (config.get<boolean>("openInEditor")) {
    await vscode.commands.executeCommand("vscode.open", uri);
  } else {
    await vscode.env.openExternal(uri);
  }
}

export function activate(context: vscode.ExtensionContext) {
  context.subscriptions.push(
    vscode.commands.registerCommand("typstRedline.showChanges", () =>
      showChanges().catch((e: Error) => vscode.window.showErrorMessage(`Typst Redline: ${e.message}`)),
    ),
  );
}

export function deactivate() {}
