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

// The file right-clicked in a menu, which VS Code passes as `uri`, or else the active editor's.
async function typstDocument(uri?: vscode.Uri) {
  const doc = uri instanceof vscode.Uri ? await vscode.workspace.openTextDocument(uri) : vscode.window.activeTextEditor?.document;
  if (!doc || doc.uri.scheme !== "file" || !doc.fileName.endsWith(".typ")) {
    throw new Error("Open or right-click a saved .typ file first.");
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
type Base = { rev: string; to?: string; ends?: string[]; what: string } | { file: string; what: string };
type Commit = { hash: string; date: string; subject: string; root: boolean };
type BaseItem = vscode.QuickPickItem & { base?: Base | "view" | "other" | "several" | "between" };

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

// One list; resolves to the accepted items, or undefined when the list is closed.
function pick<T extends vscode.QuickPickItem>(
  items: readonly T[],
  o: { title: string; placeholder: string; active?: T; many?: boolean },
): Promise<readonly T[] | undefined> {
  return new Promise((resolve) => {
    const list = vscode.window.createQuickPick<T>();
    list.title = o.title;
    list.placeholder = o.placeholder;
    list.matchOnDescription = true;
    list.canSelectMany = !!o.many;
    list.items = items;
    if (o.many) {
      list.selectedItems = items.filter((i) => i.picked);
    } else if (o.active) {
      list.activeItems = [o.active];
    }
    let accepted = false;
    list.onDidAccept(() => {
      accepted = true;
      resolve(o.many ? list.selectedItems : list.activeItems.slice(0, 1));
      list.hide();
    });
    list.onDidHide(() => {
      if (!accepted) {
        resolve(undefined);
      }
      list.dispose();
    });
    list.show();
  });
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

// The CLI follows the first parent, so every commit listed is one it accepts as a base or
// as the end of a step or of a range.
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
  const targets: BaseItem[] = [];
  let list: Commit[] = [];
  let why = noRepo;
  if (repo) {
    const [log, date] = await Promise.all([
      commits(dir, root),
      reviewed ? git(dir, "log", "-1", "--date=short", "--format=%ad", reviewed).catch(() => "") : "",
    ]);
    ({ list, why } = log);
    if (reviewed) {
      targets.push({
        label: "$(eye) Last reviewed version",
        description: date.trim(),
        base: { rev: "reviewed", what: "since the last reviewed version" },
      });
    }
    targets.push(
      ...list.map((c) => ({ ...commitItem(c), base: { rev: c.hash, what: `since ${quote(c)}` } })),
      { label: "", kind: vscode.QuickPickItemKind.Separator },
    );
    const tickable = list.filter((c) => !c.root).length;
    if (tickable > 1) {
      targets.push({
        label: "$(list-selection) Several commits...",
        description: "a new colour at each ticked commit, in the view's layout",
        base: "several",
      });
    }
    if (tickable > 0) {
      targets.push({
        label: "$(git-compare) Between two commits...",
        description: "from one commit to another, not to the file as it is now",
        base: "between",
      });
    }
  }
  targets.push({ label: "$(file) Another file...", description: "compare with a .typ file of your choice", base: "other" });
  for (;;) {
    const viewItem: BaseItem = { label: `$(settings) View: ${viewName(currentView())}`, description: "change: marked, side by side, coloured by commit", base: "view" };
    const items = [viewItem, { label: "", kind: vscode.QuickPickItemKind.Separator }, ...targets];
    const [choice] = (await pick(items, {
      title: "Typst Redline: Show changes since...",
      placeholder: `${why ? `${why} ` : ""}Compare the file as it is now with...`,
      active: targets.find((t) => t.kind !== vscode.QuickPickItemKind.Separator),
    })) ?? [undefined];
    const base = choice?.base;
    if (base === "view") {
      await pickView();
      continue;
    }
    if (base === "several" || base === "between") {
      const picked = await (base === "several" ? pickSeveral(list, dir) : pickBetween(list, dir));
      if (picked === "back") {
        continue;
      }
      return picked;
    }
    if (base !== "other") {
      return base;
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

// The parents' short hashes, which name the steps in the PDF.
async function parents(dir: string, commits: Commit[]) {
  return Promise.all(commits.map(async (c) => (await git(dir, "rev-parse", "--short", `${c.hash}^`)).trim()));
}

// Commits picked "from" include their own change, so the base is their parent. A commit
// without a parent in the repository, the first or the edge of a shallow clone, cannot be
// picked so.
async function pickSeveral(list: Commit[], dir: string): Promise<Base | "back" | undefined> {
  let items = list.filter((c) => !c.root).map(commitItem);
  for (;;) {
    const ticked = await pick(items, {
      title: "Typst Redline: colour the changes by step",
      placeholder: `Tick up to ${maxColours} commits; each one starts a new colour`,
      many: true,
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
    const commits = ticked.map((i) => i.c).reverse();
    const [rev, ...ends] = await parents(dir, commits);
    return { rev, what: `from ${quote(commits[0])} on`, ends: ends.length ? ends : undefined };
  }
}

async function pickBetween(list: Commit[], dir: string): Promise<Base | undefined> {
  const [from] = (await pick(list.filter((c) => !c.root).map(commitItem), {
    title: "Typst Redline: changes from...",
    placeholder: "Pick the oldest commit to show; its own change is included",
  })) ?? [undefined];
  if (!from) {
    return undefined;
  }
  const [to] = (await pick(list.slice(0, list.indexOf(from.c) + 1).map(commitItem), {
    title: `Typst Redline: changes from ${quote(from.c)} to...`,
    placeholder: "Pick the newest commit to show; its own change is included",
  })) ?? [undefined];
  return to && { rev: (await parents(dir, [from.c]))[0], to: to.c.hash, what: `from ${quote(from.c)} to ${quote(to.c)}` };
}

type View = { layout: "marked" | "side-by-side"; colours: "none" | "each" | number };

const layouts = [
  { layout: "marked", label: "Marked", detail: "The new version with the changes marked" },
  { layout: "side-by-side", label: "Side by side", detail: "Old and new pages next to each other (Typst 0.14 or later)" },
] as const;

function currentView(): View {
  const v = state.get<View>("view");
  const valid =
    layouts.some((l) => l.layout === v?.layout) &&
    (v?.colours === "none" || v?.colours === "each" || [2, 3, 4, 5, 6].includes(v?.colours as number));
  return valid ? v! : { layout: "marked", colours: "none" };
}

function viewName(v: View) {
  const layout = layouts.find((l) => l.layout === v.layout)!.label;
  return v.colours === "none"
    ? layout
    : `${layout}, coloured by commit${typeof v.colours === "number" ? `, ${v.colours} steps` : ""}`;
}

// Chooses and stores the view; a coloured view also chooses how the commits are coloured.
async function pickView() {
  const now = currentView();
  const items = layouts.flatMap((l) => [
    { label: l.label, detail: l.detail, layout: l.layout, coloured: false },
    { label: `${l.label}, coloured by commit`, detail: `${l.detail}, in colours by commit`, layout: l.layout, coloured: true },
  ]);
  const [view] = (await pick(items, {
    title: "Typst Redline: view",
    placeholder: "Show the changes as...",
    active: items.find((i) => i.layout === now.layout && i.coloured === (now.colours !== "none")),
  })) ?? [undefined];
  if (!view) {
    return;
  }
  let colours: View["colours"] = "none";
  if (view.coloured) {
    const choices = [
      { label: "One colour per commit", detail: "each change in the colour of the commit that made it", colours: "each" as const },
      ...[2, 3, 4, 5, 6].map((n) => ({
        label: `${n} steps`,
        detail: `the commits split into ${n} groups of about equal change`,
        colours: n,
      })),
    ];
    const [choice] = (await pick(choices, {
      title: "Typst Redline: colours",
      placeholder: "Colour each commit, or split the commits into steps",
      active: choices.find((c) => c.colours === now.colours),
    })) ?? [undefined];
    if (!choice) {
      return;
    }
    colours = choice.colours;
  }
  await state.update("view", { layout: view.layout, colours });
}

function viewArgs(view: View, base: Base, colours: boolean) {
  const layout = view.layout === "side-by-side" ? ["--layout", "side-by-side"] : [];
  if (!("rev" in base) || !(base.ends || (colours && view.colours !== "none"))) {
    return layout;
  }
  if (base.ends) {
    return [...layout, "--steps", base.ends.join(",")];
  }
  return [...layout, ...(view.colours === "each" ? ["--each-commit"] : ["--steps", String(view.colours)])];
}

// The command chooses these itself, from its lists.
const flowOptions = ["--since", "--to", "--layout", "--each-commit", "--steps"];

function checkExtraArgs(extra: string[]) {
  const end = extra.indexOf("--");
  const owned = (end < 0 ? extra : extra.slice(0, end)).filter((a) => flowOptions.includes(a));
  if (owned.length) {
    throw new Error(
      `typstRedline.extraArgs holds ${owned.join(" and ")}, which this command sets itself. ` +
        "Remove it from that setting; choose the view with the View entry of the list.",
    );
  }
}

// Why the CLI cannot colour these changes by commit, in the terms of the command, or
// undefined for other errors.
function noColours(message: string) {
  if (/has nothing to colour/.test(message)) {
    return "no commit in this range changes this document";
  }
  if (/to be an ancestor of/.test(message)) {
    return "the commit is not an earlier commit of this branch";
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

async function showChanges(clicked?: vscode.Uri) {
  const doc = await typstDocument(clicked);
  const file = doc.fileName;
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const extra = config.get<string[]>("extraArgs", []);
  checkExtraArgs(extra);
  if (!idle(file)) {
    return;
  }
  const proj = project(doc);
  const base = await pickBase(doc, proj);
  if (!base || !idle(file)) {
    return;
  }
  const view = currentView();
  const { root } = await proj;
  // Save what the run reads: open documents of the project.
  for (const d of unsaved(doc, root)) {
    await d.save();
  }
  // An old file outside the default root gets the CLI's default, the directory of each file.
  const rootArgs = "file" in base && !inside(base.file, root) && !config.get<string>("root") ? [] : ["--root", root];
  const typst = config.get<string>("typst");
  const args = (viewArgs: string[]) => [
    ...("rev" in base ? [file, "--since", base.rev, ...(base.to ? ["--to", base.to] : [])] : [base.file, file]),
    ...rootArgs,
    ...(typst ? ["--typst", typst] : []),
    ...viewArgs,
    ...extra,
  ];
  const coloured = view.colours !== "none" && !("rev" in base && base.ends);
  if (coloured && "file" in base) {
    vscode.window.showInformationMessage("Typst Redline: another file has no commits, so the changes are not coloured.");
  }
  busy.add(file);
  let pdf: string;
  try {
    pdf = await vscode.window.withProgress(
      { location: vscode.ProgressLocation.Notification, title: `Typst Redline: changes ${base.what}`, cancellable: true },
      async (_, token) => {
        try {
          return await runCli(doc, args(viewArgs(view, base, true)), token);
        } catch (e) {
          const message = (e as Error).message;
          const why = coloured && noColours(message);
          const more = coloured && /colours at most \d+ steps/.test(message);
          if (!why && !more) {
            throw e;
          }
          output.appendLine(message);
          if (more) {
            const layout = viewArgs(view, base, false);
            vscode.window.showInformationMessage(
              `Typst Redline: more than ${maxColours} colours would be needed, so the commits are coloured in ${maxColours} steps.`,
            );
            return await runCli(doc, args([...layout, "--steps", String(maxColours)]), token);
          }
          vscode.window.showInformationMessage(`Typst Redline: ${why}, so the changes are not coloured.`);
          return await runCli(doc, args(viewArgs(view, base, false)), token);
        }
      },
    );
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
  const doc = await typstDocument();
  const message = await runCli(doc, [doc.fileName, "--mark-reviewed"]);
  vscode.window.showInformationMessage(`Typst Redline: ${message.match(/^marked \S+ as reviewed/)?.[0] ?? message}`);
}

function command(name: string, body: (uri?: vscode.Uri) => Promise<void>) {
  return vscode.commands.registerCommand(name, async (uri?: vscode.Uri) => {
    try {
      await body(uri);
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
  state = context.workspaceState;
  context.subscriptions.push(
    output,
    command("typstRedline.showChanges", showChanges),
    command("typstRedline.markReviewed", markReviewed),
  );
}

export function deactivate() {
  running.forEach(killTree);
}
