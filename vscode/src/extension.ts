import * as vscode from "vscode";
import * as path from "path";
import { execFile } from "child_process";

function run(cmd: string, args: string[], cwd: string, notFound: string): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(cmd, args, { cwd, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (!err) {
        resolve(stdout);
      } else if ((err as NodeJS.ErrnoException).code === "ENOENT") {
        reject(new Error(`"${cmd}" was not found. ${notFound}`));
      } else {
        reject(new Error(stderr.trim() || err.message));
      }
    });
  });
}

async function showChanges() {
  const doc = vscode.window.activeTextEditor?.document;
  if (!doc || doc.uri.scheme !== "file" || !doc.fileName.endsWith(".typ")) {
    throw new Error("Open a .typ file first.");
  }
  const file = doc.fileName;
  const dir = path.dirname(file);
  const log = await run(
    "git",
    ["log", "-n", "40", "--date=short", "--format=%h%x09%ad%x09%s", "--", path.basename(file)],
    dir,
    "Install git and make sure it is on PATH.",
  );
  const items = log
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => {
      const [hash, date, ...subject] = l.split("\t");
      return { label: subject.join("\t"), description: `${hash}  ${date}`, rev: hash };
    });
  if (items.length === 0) {
    throw new Error("No git history for this file. Commit it first.");
  }
  const pick = await vscode.window.showQuickPick(items, {
    placeHolder: "Compare the working copy with the file as of commit…",
    matchOnDescription: true,
  });
  if (!pick) {
    return;
  }
  if (doc.isDirty) {
    await doc.save();
  }
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri)?.uri.fsPath ?? dir;
  const root = path.resolve(folder, config.get<string>("root") || "");
  const exe = config.get<string>("executable") || "typst-redline";
  const typst = config.get<string>("typst");
  const pdf = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: `Typst Redline: changes since ${pick.rev}` },
    () =>
      run(
        exe,
        [file, "--since", pick.rev, "--root", root, ...(typst ? ["--typst", typst] : [])],
        dir,
        "Install typst-redline and put it on PATH, or set typstRedline.executable to its full path.",
      ),
  );
  const uri = vscode.Uri.file(path.resolve(dir, pdf.trim()));
  if (config.get<boolean>("openInEditor")) {
    await vscode.commands.executeCommand("vscode.open", uri);
  } else {
    await vscode.env.openExternal(uri);
  }
}

export function activate(context: vscode.ExtensionContext) {
  const output = vscode.window.createOutputChannel("Typst Redline");
  context.subscriptions.push(
    output,
    vscode.commands.registerCommand("typstRedline.showChanges", async () => {
      try {
        await showChanges();
      } catch (e) {
        const lines = (e as Error).message.split("\n");
        if (lines.length === 1) {
          vscode.window.showErrorMessage(`Typst Redline: ${lines[0]}`);
          return;
        }
        // Typst diagnostics span many lines; a notification shows only the first.
        output.appendLine(lines.join("\n"));
        const show = "Show Output";
        vscode.window
          .showErrorMessage(`Typst Redline: ${lines[lines.length - 1]}`, show)
          .then((choice) => choice === show && output.show());
      }
    }),
  );
}

export function deactivate() {}
