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

// A commit base also carries the number of commits after it and whether there are
// uncommitted changes, which bound the number of colours of the per-commit views.
type Base =
  | { rev: string; label: string; commits: number; uncommitted: boolean; ends?: string[] }
  | { file: string; label: string };
type BaseItem = vscode.QuickPickItem & { base?: Base | "other" | "several" };

function unsaved(doc: vscode.TextDocument, root: string) {
  return vscode.workspace.textDocuments.filter(
    (d) => d.isDirty && d.uri.scheme === "file" && (d === doc || inside(d.fileName, root)),
  );
}

async function git(dir: string, ...args: string[]) {
  return (await run("git", args, dir, "")).stdout;
}

// The commit that Mark as reviewed recorded for the file: the CLI keeps it in a ref named
// after the file's path in the repository, percent-encoded.
async function reviewedCommit(dir: string, file: string) {
  const rel = (await git(dir, "rev-parse", "--show-prefix")).trim() + path.basename(file);
  const prefix = "refs/typst-redline/reviewed/";
  const refs = await git(dir, "for-each-ref", "--format=%(objectname) %(refname)", prefix);
  const fold = (s: string) => (process.platform === "linux" ? s : s.toLowerCase());
  for (const line of refs.split(/\r?\n/)) {
    const [hash, ref] = line.split(" ");
    try {
      if (ref && fold(decodeURIComponent(ref.slice(prefix.length))) === fold(rel)) {
        return hash;
      }
    } catch {}
  }
}

async function pickBase(doc: vscode.TextDocument, { root, repo, why }: Project): Promise<Base | undefined> {
  const file = doc.fileName;
  const dir = path.dirname(file);
  const items: BaseItem[] = [];
  let commits: BaseItem[] = [];
  if (repo) {
    try {
      const uncommitted =
        unsaved(doc, root).length > 0 || (await git(dir, "status", "--porcelain", "--", root)).trim() !== "";
      const log = await git(dir, "log", "-n", "40", "--date=short", "--format=%h%x09%ad%x09%s", "--", root);
      commits = log
        .split(/\r?\n/)
        .filter((l) => l.trim())
        .map((l, i) => {
          const [hash, date, ...subject] = l.split("\t");
          return {
            label: subject.join("\t"),
            description: `${hash}  ${date}`,
            base: { rev: hash, label: hash, commits: i, uncommitted },
          };
        });
      why = commits.length ? "" : "No commits yet.";
      const reviewed = await reviewedCommit(dir, file);
      if (reviewed) {
        const count = await git(dir, "rev-list", "--count", `${reviewed}..HEAD`, "--", root);
        items.push({
          label: "$(eye) Last reviewed version",
          description: `${reviewed.slice(0, 7)}  marked with Mark as reviewed`,
          base: { rev: "reviewed", label: "the last reviewed version", commits: Number(count), uncommitted },
        });
      }
    } catch (e) {
      why = (e as Error).message.split("\n")[0];
    }
    items.push(...commits, { label: "", kind: vscode.QuickPickItemKind.Separator });
    if (commits.length > 1) {
      items.push({
        label: "$(list-selection) Several commits...",
        description: "a colour for the changes after each ticked commit",
        base: "several",
      });
    }
  }
  items.push({ label: "$(file) Another file...", description: "compare with a .typ file of your choice", base: "other" });
  const pick = await vscode.window.showQuickPick<BaseItem>(items, {
    title: "Typst Redline: show changes since...",
    placeHolder: `${why ? `${why} ` : ""}Compare the working copy with...`,
    matchOnDescription: true,
  });
  const choice = pick?.base;
  if (choice === "several") {
    return pickSeveral(commits);
  }
  if (choice !== "other") {
    return choice;
  }
  const other = await vscode.window.showOpenDialog({
    title: "Old version to compare with",
    defaultUri: vscode.Uri.file(dir),
    filters: { Typst: ["typ"] },
  });
  return other && { file: other[0].fsPath, label: path.basename(other[0].fsPath) };
}

// The most steps the CLI has colours for.
const maxColours = 6;

// Ticked commits split the changes since the oldest of them into steps, each in its own
// colour: a step ends at each later ticked commit, and the last one at the working copy.
async function pickSeveral(commits: BaseItem[]): Promise<Base | undefined> {
  const ticked = await vscode.window.showQuickPick(commits, {
    title: "Typst Redline: changes since the oldest ticked commit",
    placeHolder: `Tick up to ${maxColours} commits; the changes after each one get a new colour`,
    canPickMany: true,
    matchOnDescription: true,
  });
  if (!ticked?.length) {
    return undefined;
  }
  const [oldest, ...later] = ticked
    .map((item) => item.base as Base & { rev: string; commits: number })
    .sort((a, b) => b.commits - a.commits);
  return later.length ? { ...oldest, ends: later.map((b) => b.rev) } : oldest;
}

type View = { id: string; label: string; sideBySide: boolean; byCommit: boolean };

const views: View[] = [
  { id: "marked", label: "Marked PDF", sideBySide: false, byCommit: false },
  { id: "side-by-side", label: "Side by side", sideBySide: true, byCommit: false },
  { id: "by-commit", label: "Coloured by commit", sideBySide: false, byCommit: true },
  { id: "side-by-side-by-commit", label: "Side by side, coloured by commit", sideBySide: true, byCommit: true },
];

function colours(base: Base) {
  if (!("rev" in base)) {
    return 0;
  }
  return base.ends ? base.ends.length + 1 : base.commits + Number(base.uncommitted);
}

function viewDetail(view: View, base: Base) {
  const layout = view.sideBySide
    ? "Changed pages of the old and the new version next to each other (Typst 0.14 or later)"
    : "The new version with the changes marked";
  if (!view.byCommit || !("rev" in base)) {
    return layout;
  }
  if (base.ends) {
    return `${layout}, in a new colour after each ticked commit`;
  }
  const n = base.commits;
  const what = `${n} commit${n === 1 ? "" : "s"} since ${base.label}${base.uncommitted ? " and the uncommitted changes" : ""}`;
  return colours(base) > maxColours
    ? `${layout}, in ${maxColours} colours for groups of the ${what}`
    : `${layout}, each in the colour of the commit that made it: ${what}`;
}

function viewArgs(view: View, base: Base) {
  const layout = view.sideBySide ? ["--layout", "side-by-side"] : [];
  if (!view.byCommit || !("rev" in base)) {
    return layout;
  }
  const steps = base.ends
    ? ["--steps", base.ends.join(",")]
    : colours(base) > maxColours
      ? ["--steps", String(maxColours)]
      : ["--each-commit"];
  return [...layout, ...steps];
}

// The per-commit views need a git comparison with at least two commits or steps to colour;
// ticked commits always take one of them.
function availableViews(base: Base) {
  const byCommit = colours(base) > 1;
  const ticked = "rev" in base && !!base.ends;
  return views.filter((v) => (v.byCommit ? byCommit : !ticked));
}

// The view with this id, or else the one with the same layout.
function closest(available: View[], id: string | undefined) {
  const want = views.find((v) => v.id === id);
  return available.find((v) => v === want) ?? available.find((v) => v.sideBySide === want?.sideBySide);
}

async function pickView(base: Base, config: vscode.WorkspaceConfiguration) {
  const available = availableViews(base);
  const setting = config.get<string>("view", "ask");
  if (setting !== "ask") {
    return closest(available, setting) ?? available[0];
  }
  const last = closest(available, state.get<string>("view"));
  const ordered = last ? [last, ...available.filter((v) => v !== last)] : available;
  const pick = await vscode.window.showQuickPick(
    ordered.map((view) => ({ label: view.label, detail: viewDetail(view, base), view })),
    { title: `Typst Redline: changes since ${base.label}`, placeHolder: "Show the changes as..." },
  );
  if (pick) {
    await state.update("view", pick.view.id);
  }
  return pick?.view;
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
  const base = await pickBase(doc, proj);
  if (!base) {
    return;
  }
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const view = await pickView(base, config);
  if (!view) {
    return;
  }
  const { root } = proj;
  // Save what the run reads: open documents of the project.
  for (const d of unsaved(doc, root)) {
    await d.save();
  }
  // An old file outside the default root gets the CLI's default, the directory of each file.
  const rootArgs = "file" in base && !inside(base.file, root) && !config.get<string>("root") ? [] : ["--root", root];
  const typst = config.get<string>("typst");
  const args = [
    ...("rev" in base ? [file, "--since", base.rev] : [base.file, file]),
    ...rootArgs,
    ...(typst ? ["--typst", typst] : []),
    ...viewArgs(view, base),
    ...config.get<string[]>("extraArgs", []),
  ];
  const pdf = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Typst Redline: changes since ${base.label}`,
      cancellable: true,
    },
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
