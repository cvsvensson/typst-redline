import * as vscode from "vscode";
import * as path from "path";
import { execFile } from "child_process";

let output: vscode.OutputChannel;
let state: vscode.Memento;

const exeHint = "Install typst-redline and put it on PATH, or set typstRedline.executable to its full path.";

function run(
  cmd: string,
  args: string[],
  cwd: string,
  notFound: string,
  token?: vscode.CancellationToken,
): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = execFile(cmd, args, { cwd, maxBuffer: 16 * 1024 * 1024 }, (err, stdout, stderr) => {
      if (token?.isCancellationRequested) {
        reject(new vscode.CancellationError());
      } else if (!err) {
        resolve({ stdout, stderr });
      } else if ((err as NodeJS.ErrnoException).code === "ENOENT") {
        reject(new Error(`"${cmd}" was not found. ${notFound}`));
      } else {
        reject(new Error(stderr.trim() || err.message));
      }
    });
    token?.onCancellationRequested(() => child.kill());
  });
}

function runCli(doc: vscode.TextDocument, args: string[], token?: vscode.CancellationToken) {
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const exe = config.get<string>("executable") || "typst-redline";
  output.appendLine(`> ${exe} ${args.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ")}`);
  return run(exe, args, path.dirname(doc.fileName), exeHint, token).then(({ stdout, stderr }) => {
    if (stderr.trim()) {
      output.appendLine(stderr.trim());
      notify(vscode.window.showWarningMessage, stderr);
    }
    return stdout.trim();
  });
}

// The CLI's message is the first "error:" line; typst diagnostics and usage text follow it.
function notify(show: (message: string, ...items: string[]) => Thenable<string | undefined>, text: string) {
  const lines = text.trim().split("\n");
  const main = lines.find((l) => l.startsWith("error:")) ?? lines[0];
  const more = lines.length > 1 ? ["Show Output"] : [];
  show(`Typst Redline: ${main.replace(/^error: /, "")}`, ...more).then((choice) => choice && output.show(true));
}

function activeTypstDocument() {
  const doc = vscode.window.activeTextEditor?.document;
  if (!doc || doc.uri.scheme !== "file" || !doc.fileName.endsWith(".typ")) {
    throw new Error("Open a saved .typ file first.");
  }
  return doc;
}

type Base = { rev: string } | { file: string };
type BaseItem = vscode.QuickPickItem & { base?: Base | "other" };

async function pickBase(file: string): Promise<Base | undefined> {
  let items: BaseItem[] = [];
  let why = "";
  try {
    const { stdout } = await run(
      "git",
      ["log", "-n", "40", "--date=short", "--format=%h%x09%ad%x09%s", "--", path.basename(file)],
      path.dirname(file),
      "Install git and put it on PATH.",
    );
    items = stdout
      .split("\n")
      .filter((l) => l.trim())
      .map((l) => {
        const [hash, date, ...subject] = l.split("\t");
        return { label: subject.join("\t"), description: `${hash}  ${date}`, base: { rev: hash } };
      });
    why = items.length ? "" : "This file has no commits yet. ";
    items.push(
      { label: "", kind: vscode.QuickPickItemKind.Separator },
      {
        label: "$(eye) Last reviewed version",
        description: "set with Typst Redline: Mark as reviewed",
        base: { rev: "reviewed" },
      },
    );
  } catch (e) {
    why = `${(e as Error).message.split("\n")[0]} `;
  }
  items.push({ label: "$(file) Another file…", description: "compare with a .typ file of your choice", base: "other" });
  const pick = await vscode.window.showQuickPick(items, {
    title: "Typst Redline: show changes since…",
    placeHolder: `${why}Compare the working copy with…`,
    matchOnDescription: true,
  });
  if (pick?.base !== "other") {
    return pick?.base;
  }
  const other = await vscode.window.showOpenDialog({
    title: "Old version to compare with",
    defaultUri: vscode.Uri.file(path.dirname(file)),
    filters: { Typst: ["typ"] },
  });
  return other && { file: other[0].fsPath };
}

type OptionItem = vscode.QuickPickItem & { args: string[]; git?: boolean };

const options: OptionItem[] = [
  {
    label: "Side by side",
    detail: "Changed pages of the old and the new version next to each other (Typst 0.14 or later)",
    args: ["--layout", "side-by-side"],
  },
  {
    label: "Colour by commit",
    detail: "Each change in the colour of the commit that made it, for up to 6 commits",
    args: ["--each-commit"],
    git: true,
  },
  {
    label: "Colour by step…",
    detail: "Group the commits into 2 to 6 steps, each in its own colour",
    args: ["--steps"],
    git: true,
  },
  {
    label: "No moves",
    detail: "Show moved text as deleted at its old place and inserted at its new place",
    args: ["--no-moves"],
  },
];

async function pickOptions(git: boolean): Promise<string[] | undefined> {
  const last = state.get<string[]>("options", []);
  const items = options
    .filter((o) => git || !o.git)
    .map((o) => ({ ...o, picked: last.includes(o.label) }));
  const picked = await vscode.window.showQuickPick(items, {
    title: "Typst Redline: options (Enter to run)",
    placeHolder: "Tick options, or press Enter for the plain marked-up PDF",
    canPickMany: true,
  });
  if (!picked) {
    return;
  }
  const hidden = last.filter((label) => !items.some((o) => o.label === label));
  await state.update("options", [...hidden, ...picked.map((o) => o.label)]);
  const args = picked.flatMap((o) => o.args);
  if (args.includes("--steps")) {
    const steps = await vscode.window.showInputBox({
      title: "Typst Redline: steps",
      prompt: "A number of steps from 2 to 6, or the revisions where steps end, separated by commas",
      value: state.get<string>("steps", "3"),
    });
    if (!steps) {
      return;
    }
    await state.update("steps", steps);
    args.splice(args.indexOf("--steps") + 1, 0, steps);
  }
  return args;
}

function hasPdfEditor() {
  return vscode.extensions.all.some((e) =>
    (e.packageJSON.contributes?.customEditors ?? []).some((editor: any) =>
      (editor.selector ?? []).some((s: any) => /\.pdf$/i.test(s.filenamePattern ?? "")),
    ),
  );
}

async function showChanges() {
  const doc = activeTypstDocument();
  const file = doc.fileName;
  const base = await pickBase(file);
  if (!base) {
    return;
  }
  const optionArgs = await pickOptions("rev" in base);
  if (!optionArgs) {
    return;
  }
  for (const d of vscode.workspace.textDocuments) {
    if (d.isDirty && d.fileName.endsWith(".typ")) {
      await d.save();
    }
  }
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri)?.uri.fsPath ?? path.dirname(file);
  const root = path.resolve(folder, config.get<string>("root") || "");
  const rel = "file" in base ? path.relative(root, base.file) : "";
  // An old file outside the default root gets the CLI's default, the directory of each file.
  const outside = rel.startsWith("..") || path.isAbsolute(rel);
  const rootArgs = outside && !config.get<string>("root") ? [] : ["--root", root];
  const typst = config.get<string>("typst");
  const args = [
    ...("rev" in base ? [file, "--since", base.rev] : [base.file, file]),
    ...rootArgs,
    ...(typst ? ["--typst", typst] : []),
    ...optionArgs,
    ...config.get<string[]>("extraArgs", []),
  ];
  const title = "rev" in base ? `changes since ${base.rev}` : `changes since ${path.basename(base.file)}`;
  const pdf = await vscode.window.withProgress(
    { location: vscode.ProgressLocation.Notification, title: `Typst Redline: ${title}`, cancellable: true },
    (_, token) => runCli(doc, args, token),
  );
  const uri = vscode.Uri.file(path.resolve(path.dirname(file), pdf));
  const open = config.get<string>("openPdf");
  if (open === "editor" || (open !== "external" && hasPdfEditor())) {
    await vscode.commands.executeCommand("vscode.open", uri, {
      viewColumn: vscode.ViewColumn.Beside,
      preserveFocus: true,
    });
  } else {
    await vscode.env.openExternal(uri);
  }
}

async function markReviewed() {
  const doc = activeTypstDocument();
  const message = await runCli(doc, [doc.fileName, "--mark-reviewed"]);
  vscode.window.showInformationMessage(`Typst Redline: ${message}`);
}

function command(name: string, body: () => Promise<void>) {
  return vscode.commands.registerCommand(name, async () => {
    try {
      await body();
    } catch (e) {
      if (e instanceof vscode.CancellationError) {
        return;
      }
      output.appendLine((e as Error).message);
      notify(vscode.window.showErrorMessage, (e as Error).message);
    }
  });
}

export function activate(context: vscode.ExtensionContext) {
  output = vscode.window.createOutputChannel("Typst Redline");
  state = context.globalState;
  context.subscriptions.push(
    output,
    command("typstRedline.showChanges", showChanges),
    command("typstRedline.markReviewed", markReviewed),
  );
}

export function deactivate() {}
