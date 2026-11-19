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

// `what` completes "changes ..." in titles and messages. A commit base runs from `rev` to
// `to`, or to the file as it is now; `ends` are the revisions where ticked colour steps end.
// `most` bounds the number of steps one colour per commit takes: the commits the CLI can
// colour, and the uncommitted changes.
type Base =
  | { rev: string; to?: string; ends?: string[]; most: number; what: string }
  | { file: string; what: string };
type Commit = { hash: string; date: string; subject: string; root: boolean };
type BaseItem = vscode.QuickPickItem & { base?: Base | "other" | "several" | "between" };

function unsaved(doc: vscode.TextDocument, root: string) {
  return vscode.workspace.textDocuments.filter(
    (d) => d.isDirty && d.uri.scheme === "file" && (d === doc || inside(d.fileName, root)),
  );
}

async function git(dir: string, ...args: string[]) {
  return (await run("git", ["-c", "log.showSignature=false", ...args], dir, "")).stdout;
}

function exe(doc: vscode.TextDocument) {
  return vscode.workspace.getConfiguration("typstRedline", doc.uri).get<string>("executable") || "typst-redline";
}

// The commit that the CLI compares with for --since reviewed, if there is one.
async function reviewedCommit(doc: vscode.TextDocument) {
  try {
    const dir = path.dirname(doc.fileName);
    const hash = (await run(exe(doc), [doc.fileName, "--show-reviewed"], dir, "")).stdout.trim();
    return hash || undefined;
  } catch {
    return undefined;
  }
}

// The CLI follows the first parent, so every commit listed is one it accepts as a base, as
// the end of a step or of a range, and colours at most the commits listed after a base.
async function commits(dir: string, root: string): Promise<{ list: Commit[]; why: string }> {
  try {
    const log = await git(dir, "log", "--first-parent", "-n", "40", "--date=short", "--format=%h%x09%p%x09%ad%x09%s", "--", root);
    const list = log
      .split(/\r?\n/)
      .filter((l) => l.trim())
      .map((l) => {
        const [hash, parents, date, ...subject] = l.split("\t");
        return { hash, date, subject: subject.join("\t"), root: !parents };
      });
    return { list, why: list.length ? "" : "No commits touch this folder." };
  } catch (e) {
    const message = (e as Error).message;
    if (/does not have any commits/.test(message)) {
      return { list: [], why: "No commits yet." };
    }
    output.appendLine(message);
    return { list: [], why: "Could not read the git history; see the Typst Redline output." };
  }
}

const quote = (c: Commit) => `"${c.subject}"`;
const commitItem = (c: Commit) => ({ label: c.subject, description: `${c.date}  ${c.hash}`, c });

async function pickBase(doc: vscode.TextDocument, proj: Promise<Project>): Promise<Base | undefined> {
  const dir = path.dirname(doc.fileName);
  const [{ root, repo, why: noRepo }, reviewed] = await Promise.all([proj, reviewedCommit(doc)]);
  const items: BaseItem[] = [];
  let list: Commit[] = [];
  let why = noRepo;
  if (repo) {
    const [log, date, after] = await Promise.all([
      commits(dir, root),
      reviewed ? git(dir, "log", "-1", "--date=short", "--format=%ad", reviewed).catch(() => "") : "",
      reviewed
        ? git(dir, "rev-list", "--first-parent", "--count", `${reviewed}..HEAD`, "--", root).catch(() => "")
        : "",
    ]);
    ({ list, why } = log);
    if (reviewed) {
      items.push({
        label: "$(eye) Last reviewed version",
        description: `${date.trim()} \u00b7 marked with Mark as reviewed`,
        base: { rev: reviewed, most: (after ? Number(after) : Infinity) + 1, what: "since the last reviewed version" },
      });
    }
    items.push(
      ...list.map((c, i) => ({
        ...commitItem(c),
        base: { rev: c.hash, most: i + 1, what: `since ${quote(c)}` },
      })),
      { label: "", kind: vscode.QuickPickItemKind.Separator },
    );
    const tickable = list.filter((c) => !c.root).length;
    if (tickable > 1) {
      items.push({
        label: "$(list-selection) Several commits...",
        description: "tick commits to colour the changes in steps",
        base: "several",
      });
    }
    if (tickable > 0) {
      items.push({
        label: "$(git-compare) Between two commits...",
        description: "from one commit to another, not to the file as it is now",
        base: "between",
      });
    }
  }
  items.push({ label: "$(file) Another file...", description: "compare with a .typ file of your choice", base: "other" });
  for (;;) {
    const pick = await vscode.window.showQuickPick<BaseItem>(items, {
      title: "Typst Redline: Show changes since...",
      placeHolder: `${why ? `${why} ` : ""}Compare the file as it is now with...`,
      matchOnDescription: true,
    });
    const choice = pick?.base;
    if (choice === "several" || choice === "between") {
      const base = await (choice === "several" ? pickSeveral(list) : pickBetween(list));
      if (base === "back") {
        continue;
      }
      return base;
    }
    if (choice !== "other") {
      return choice;
    }
    const other = await vscode.window.showOpenDialog({
      title: "Old version to compare with",
      defaultUri: vscode.Uri.file(dir),
      filters: { Typst: ["typ"] },
    });
    return other && { file: other[0].fsPath, what: `since ${path.basename(other[0].fsPath)}` };
  }
}

// The most steps the CLI has colours for.
const maxColours = 6;

// Commits picked "from" include their own change, so the base is their parent. A commit
// without a parent in the repository, the first or the edge of a shallow clone, cannot be
// picked so.
async function pickSeveral(list: Commit[]): Promise<Base | "back" | undefined> {
  let items = list.filter((c) => !c.root).map(commitItem);
  for (;;) {
    const ticked = await vscode.window.showQuickPick(items, {
      title: "Typst Redline: colour the changes by step",
      placeHolder: `Tick up to ${maxColours} commits; each one starts a new colour`,
      canPickMany: true,
      matchOnDescription: true,
    });
    if (!ticked) {
      return undefined;
    }
    if (!ticked.length) {
      return "back";
    }
    if (ticked.length > maxColours) {
      vscode.window.showWarningMessage(`Typst Redline: tick at most ${maxColours} commits.`);
      items = items.map((i) => ({ ...i, picked: ticked.includes(i) }));
      continue;
    }
    const [oldest, ...later] = ticked.map((i) => i.c).reverse();
    return {
      rev: `${oldest.hash}^`,
      what: `from ${quote(oldest)} on`,
      most: list.indexOf(oldest) + 2,
      ends: later.length ? later.map((c) => `${c.hash}^`) : undefined,
    };
  }
}

async function pickBetween(list: Commit[]): Promise<Base | "back" | undefined> {
  const from = await vscode.window.showQuickPick(list.filter((c) => !c.root).map(commitItem), {
    title: "Typst Redline: changes from...",
    placeHolder: "Pick the oldest commit to show; its own change is included",
    matchOnDescription: true,
  });
  if (!from) {
    return undefined;
  }
  const start = list.indexOf(from.c);
  const to = await vscode.window.showQuickPick(list.slice(0, start + 1).map(commitItem), {
    title: `Typst Redline: changes from ${quote(from.c)} to...`,
    placeHolder: "Pick the newest commit to show; its own change is included",
    matchOnDescription: true,
  });
  if (!to) {
    return undefined;
  }
  return {
    rev: `${from.c.hash}^`,
    to: to.c.hash,
    most: start - list.indexOf(to.c) + 1,
    what: `from ${quote(from.c)} to ${quote(to.c)}`,
  };
}

type View = { id: string; label: string; sideBySide: boolean; byCommit: boolean };

const views: View[] = [
  { id: "marked", label: "Marked", sideBySide: false, byCommit: false },
  { id: "side-by-side", label: "Side by side", sideBySide: true, byCommit: false },
  { id: "by-commit", label: "Marked, coloured by commit", sideBySide: false, byCommit: true },
  { id: "side-by-side-by-commit", label: "Side by side, coloured by commit", sideBySide: true, byCommit: true },
];

function viewDetail(view: View, base: Base) {
  const layout = view.sideBySide ? "Old and new pages next to each other" : "The new version with the changes marked";
  if (!view.byCommit) {
    return view.sideBySide ? `${layout} (Typst 0.14 or later)` : layout;
  }
  return "ends" in base && base.ends
    ? `${layout}, a new colour at each ticked commit`
    : `${layout}, each change in the colour of its commit`;
}

// Only a git comparison can be coloured by commit; ticked commits ask for colours.
function availableViews(base: Base) {
  const ticked = "rev" in base && !!base.ends;
  return views.filter((v) => (v.byCommit ? "rev" in base : !ticked));
}

// The view with this id, or else the one with the same layout.
function closest(available: View[], id: string | undefined) {
  const want = views.find((v) => v.id === id);
  return available.find((v) => v === want) ?? available.find((v) => v.sideBySide === want?.sideBySide);
}

// The colour options of a coloured view: one colour per commit while the CLI has enough
// colours, or a number of steps.
function stepChoices(base: { most: number }) {
  const choices: { label: string; detail: string; args: string[] }[] = [];
  if (base.most <= maxColours) {
    choices.push({
      label: "One colour per commit",
      detail: "each change in the colour of the commit that made it",
      args: ["--each-commit"],
    });
  }
  for (let n = 2; n <= Math.min(maxColours, base.most - 1); n++) {
    choices.push({
      label: `${n} steps`,
      detail: `the commits split into ${n} groups of about equal change`,
      args: ["--steps", String(n)],
    });
  }
  return choices;
}

// Returns the CLI's arguments for the view, or undefined if the user cancelled.
async function pickView(base: Base, config: vscode.WorkspaceConfiguration): Promise<string[] | undefined> {
  const available = availableViews(base);
  const setting = config.get<string>("view", "ask");
  let view: View | undefined;
  if (setting !== "ask") {
    view = closest(available, setting) ?? available[0];
  } else {
    const last = closest(available, state.get<string>("view"));
    const ordered = last ? [last, ...available.filter((v) => v !== last)] : available;
    const pick = await vscode.window.showQuickPick(
      ordered.map((view) => ({ label: view.label, detail: viewDetail(view, base), view })),
      { title: `Typst Redline: changes ${base.what}`, placeHolder: "Show the changes as..." },
    );
    if (!pick) {
      return undefined;
    }
    view = pick.view;
    await state.update("view", view.id);
  }
  const layout = view.sideBySide ? ["--layout", "side-by-side"] : [];
  if (!view.byCommit || !("rev" in base)) {
    return layout;
  }
  if (base.ends) {
    return [...layout, "--steps", base.ends.join(",")];
  }
  const choices = stepChoices(base);
  // The first choice is one colour per commit, or else the most steps.
  const fallback = choices[0].args[0] === "--each-commit" ? choices[0] : choices[choices.length - 1];
  if (setting !== "ask" || base.most <= 2) {
    return [...layout, ...fallback.args];
  }
  const last = choices.find((c) => c.label === state.get<string>("steps")) ?? fallback;
  const pick = await vscode.window.showQuickPick([last, ...choices.filter((c) => c !== last)], {
    title: `Typst Redline: colours for the changes ${base.what}`,
    placeHolder: "Colour each commit, or split the commits into steps",
  });
  if (!pick) {
    return undefined;
  }
  await state.update("steps", pick.label);
  return [...layout, ...pick.args];
}

// The command chooses these itself, from the list or typstRedline.view.
const flowOptions = ["--since", "--to", "--layout", "--each-commit", "--steps"];

function checkExtraArgs(extra: string[]) {
  const end = extra.indexOf("--");
  const owned = (end < 0 ? extra : extra.slice(0, end)).filter((a) => flowOptions.includes(a));
  if (owned.length) {
    throw new Error(
      `typstRedline.extraArgs holds ${owned.join(" and ")}, which this command sets itself. ` +
        "Remove it from that setting; choose the view in the list or with typstRedline.view.",
    );
  }
}

// What the CLI says about options the user did not type, in the terms of the commands.
function plain(message: string, base: Base) {
  if (/has nothing to colour/.test(message)) {
    return `Nothing to colour: no commit ${base.what} changes this document.`;
  }
  if (/to be an ancestor of/.test(message)) {
    return `The changes ${base.what} cannot be coloured by commit: that commit is not an earlier commit of this branch.`;
  }
}

const busy = new Set<string>();

function idle(file: string) {
  if (busy.has(file)) {
    vscode.window.showWarningMessage("Typst Redline is still running for this file.");
  }
  return !busy.has(file);
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
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const extra = config.get<string[]>("extraArgs", []);
  checkExtraArgs(extra);
  if (!idle(file)) {
    return;
  }
  const proj = project(doc);
  const base = await pickBase(doc, proj);
  if (!base) {
    return;
  }
  const viewArgs = await pickView(base, config);
  if (!viewArgs || !idle(file)) {
    return;
  }
  const { root } = await proj;
  // Save what the run reads: open documents of the project.
  for (const d of unsaved(doc, root)) {
    await d.save();
  }
  // An old file outside the default root gets the CLI's default, the directory of each file.
  const rootArgs = "file" in base && !inside(base.file, root) && !config.get<string>("root") ? [] : ["--root", root];
  const typst = config.get<string>("typst");
  const args = [
    ...("rev" in base ? [file, "--since", base.rev, ...(base.to ? ["--to", base.to] : [])] : [base.file, file]),
    ...rootArgs,
    ...(typst ? ["--typst", typst] : []),
    ...viewArgs,
    ...extra,
  ];
  busy.add(file);
  let pdf: string;
  try {
    pdf = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Typst Redline: changes ${base.what}`, cancellable: true },
      (_, token) => runCli(doc, args, token),
    );
  } catch (e) {
    const message = e instanceof Error && plain(e.message, base);
    if (message) {
      output.appendLine(e.message);
      throw new Error(message);
    }
    throw e;
  } finally {
    busy.delete(file);
  }
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
  vscode.window.showInformationMessage(`Typst Redline: ${message.match(/^marked \S+ as reviewed/)?.[0] ?? message}`);
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
