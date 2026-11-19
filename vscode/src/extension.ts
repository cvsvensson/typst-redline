import * as vscode from "vscode";
import * as path from "path";
import { ChildProcess, execFile, spawn } from "child_process";

let output: vscode.OutputChannel;
let state: vscode.Memento;

const running = new Set<ChildProcess>();

// The CLI starts typst as a child, so cancelling ends the whole process tree, not only the CLI.
function killTree(child: ChildProcess) {
  if (child.pid === undefined) {
    return;
  }
  if (process.platform === "win32") {
    execFile("taskkill", ["/pid", String(child.pid), "/T", "/F"], () => {});
  } else {
    try {
      process.kill(-child.pid, "SIGTERM");
    } catch {}
  }
}

const exeHint = "Install typst-redline and put it on PATH, or set typstRedline.executable to its full path.";

function run(
  cmd: string,
  args: string[],
  cwd: string,
  notFound: string,
  token?: vscode.CancellationToken,
): Promise<{ stdout: string; stderr: string }> {
  return new Promise((resolve, reject) => {
    // Its own process group on Unix, so that the signal reaches the children too.
    const child = spawn(cmd, args, { cwd, detached: process.platform !== "win32" });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8").on("data", (d) => (stdout += d));
    child.stderr.setEncoding("utf8").on("data", (d) => (stderr += d));
    const finish = (err?: Error, code?: number | null) => {
      running.delete(child);
      if (token?.isCancellationRequested) {
        reject(new vscode.CancellationError());
      } else if ((err as NodeJS.ErrnoException)?.code === "ENOENT") {
        reject(new Error(`"${cmd}" was not found. ${notFound}`));
      } else if (err || code !== 0) {
        reject(new Error(stderr.trim() || err?.message || `${cmd} exited with code ${code}`));
      } else {
        resolve({ stdout, stderr });
      }
    };
    child.on("error", (err) => finish(err));
    child.on("close", (code) => finish(undefined, code));
    running.add(child);
    token?.onCancellationRequested(() => killTree(child));
  });
}

function runCli(doc: vscode.TextDocument, args: string[], token?: vscode.CancellationToken) {
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const exe = config.get<string>("executable") || "typst-redline";
  output.appendLine(`> ${exe} ${args.map((a) => (/\s/.test(a) ? `"${a}"` : a)).join(" ")}`);
  return run(exe, args, path.dirname(doc.fileName), exeHint, token).then(({ stdout, stderr }) => {
    if (stderr.trim()) {
      output.appendLine(stderr.trim());
    }
    // Hints name command-line flags, which mean little in the editor; they stay in the output.
    const shown = stderr.split("\n").find((l) => /^(warning|note):/.test(l));
    if (shown) {
      notify(vscode.window.showWarningMessage, shown, true);
    }
    return stdout.trim();
  });
}

function notify(
  show: (message: string, ...items: string[]) => Thenable<string | undefined>,
  message: string,
  more: boolean,
) {
  show(`Typst Redline: ${message.replace(/^(error|warning|note): /, "")}`, ...(more ? ["Show Output"] : [])).then(
    (choice) => choice && output.show(true),
  );
}

function inside(file: string, dir: string) {
  const rel = path.relative(dir, file);
  return !rel.startsWith("..") && !path.isAbsolute(rel);
}

function activeTypstDocument() {
  const doc = vscode.window.activeTextEditor?.document;
  if (!doc || doc.uri.scheme !== "file" || !doc.fileName.endsWith(".typ")) {
    throw new Error("Open a saved .typ file first.");
  }
  return doc;
}

type Project = { root: string; repo?: string; why: string };

async function project(doc: vscode.TextDocument): Promise<Project> {
  const folder = vscode.workspace.getWorkspaceFolder(doc.uri)?.uri.fsPath ?? path.dirname(doc.fileName);
  let repo: string | undefined;
  let why = "";
  try {
    const { stdout } = await run(
      "git",
      ["rev-parse", "--show-toplevel"],
      path.dirname(doc.fileName),
      "Install git and put it on PATH.",
    );
    repo = path.resolve(stdout.trim());
  } catch (e) {
    const message = (e as Error).message;
    why = message.includes("was not found") ? message : "Not in a git repository.";
  }
  const setting = vscode.workspace.getConfiguration("typstRedline", doc.uri).get<string>("root");
  // A git revision holds only files of its repository, so the default root must not lie above it.
  const root = setting ? path.resolve(folder, setting) : repo && !inside(folder, repo) ? repo : folder;
  return { root, repo, why };
}

type Base = { rev: string } | { file: string };
type BaseItem = vscode.QuickPickItem & { base?: Base | "other" | "options" };

async function pickBase(file: string, { root, repo, why }: Project): Promise<Base | undefined> {
  let items: BaseItem[] = [];
  if (repo) {
    try {
      const { stdout } = await run(
        "git",
        ["log", "-n", "40", "--date=short", "--format=%h%x09%ad%x09%s", "--", root],
        path.dirname(file),
        "",
      );
      items = stdout
        .split("\n")
        .filter((l) => l.trim())
        .map((l) => {
          const [hash, date, ...subject] = l.split("\t");
          return { label: subject.join("\t"), description: `${hash}  ${date}`, base: { rev: hash } };
        });
      why = items.length ? "" : "No commits yet.";
    } catch (e) {
      why = (e as Error).message.split("\n")[0];
    }
    items.push(
      { label: "", kind: vscode.QuickPickItemKind.Separator },
      {
        label: "$(eye) Last reviewed version",
        description: "the commit marked with Typst Redline: Mark as reviewed",
        base: { rev: "reviewed" },
      },
    );
  }
  items.push({ label: "$(file) Another file…", description: "compare with a .typ file of your choice", base: "other" });
  for (;;) {
    const current = selectedOptions(!!repo).map((o) => o.label);
    const pick = await vscode.window.showQuickPick<BaseItem>(
      [...items, { label: "$(settings-gear) Options…", description: current.join(", ") || "none", base: "options" }],
      {
        title: "Typst Redline: show changes since…",
        placeHolder: `${why ? `${why} ` : ""}Compare the working copy with…`,
        matchOnDescription: true,
      },
    );
    const choice = pick?.base;
    if (choice === "options") {
      await pickOptions(!!repo);
      continue;
    }
    if (choice !== "other") {
      return choice;
    }
    const other = await vscode.window.showOpenDialog({
      title: "Old version to compare with",
      defaultUri: vscode.Uri.file(path.dirname(file)),
      filters: { Typst: ["typ"] },
    });
    return other && { file: other[0].fsPath };
  }
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
    label: "Colour by step",
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

function selectedOptions(git: boolean) {
  const labels = state.get<string[]>("options", []);
  return options.filter((o) => labels.includes(o.label) && (git || !o.git));
}

function optionArgs(git: boolean) {
  return selectedOptions(git).flatMap((o) => (o.args[0] === "--steps" ? ["--steps", state.get("steps", "3")] : o.args));
}

async function pickOptions(git: boolean) {
  const last = state.get<string[]>("options", []);
  const items = options.filter((o) => git || !o.git).map((o) => ({ ...o, picked: last.includes(o.label) }));
  const picked = await vscode.window.showQuickPick(items, {
    title: "Typst Redline: options",
    placeHolder: "Tick the options for this and later runs",
    canPickMany: true,
  });
  if (!picked) {
    return;
  }
  const labels = picked.map((o) => o.label);
  if (labels.includes("Colour by commit") && labels.includes("Colour by step")) {
    vscode.window.showWarningMessage("Typst Redline: pick either Colour by commit or Colour by step.");
    return;
  }
  if (labels.includes("Colour by step")) {
    const steps = await vscode.window.showInputBox({
      title: "Typst Redline: steps",
      prompt: "A number of steps from 2 to 6, or the revisions where steps end, separated by commas",
      value: state.get<string>("steps", "3"),
    });
    if (!steps) {
      return;
    }
    await state.update("steps", steps);
  }
  const hidden = last.filter((label) => !items.some((o) => o.label === label));
  await state.update("options", [...hidden, ...labels]);
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
  const proj = await project(doc);
  const base = await pickBase(file, proj);
  if (!base) {
    return;
  }
  const { root } = proj;
  // Save what the run reads: open documents of the project.
  for (const d of vscode.workspace.textDocuments) {
    if (d.isDirty && d.uri.scheme === "file" && (d === doc || inside(d.fileName, root))) {
      await d.save();
    }
  }
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  // An old file outside the default root gets the CLI's default, the directory of each file.
  const rootArgs = "file" in base && !inside(base.file, root) && !config.get<string>("root") ? [] : ["--root", root];
  const typst = config.get<string>("typst");
  const args = [
    ...("rev" in base ? [file, "--since", base.rev] : [base.file, file]),
    ...rootArgs,
    ...(typst ? ["--typst", typst] : []),
    ...optionArgs("rev" in base),
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
      const message = (e as Error).message;
      output.appendLine(message);
      // The CLI's message is the first "error:" line; typst diagnostics and usage text follow it.
      const lines = message.trim().split("\n");
      notify(vscode.window.showErrorMessage, lines.find((l) => l.startsWith("error:")) ?? lines[0], lines.length > 1);
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

export function deactivate() {
  running.forEach(killTree);
}
