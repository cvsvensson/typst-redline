import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";
import { ChildProcess, execFile, spawn } from "child_process";

let output: vscode.OutputChannel;
let state: vscode.Memento;
let extensionDir = "";

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

// The setting if the user set it, else the binary in the extension's bin folder, else PATH.
export function executable(config: vscode.WorkspaceConfiguration, dir = extensionDir) {
  const set = config.inspect<string>("executable");
  const user = set?.workspaceFolderValue || set?.workspaceValue || set?.globalValue;
  const bundled = path.join(dir, "bin", process.platform === "win32" ? "typst-redline.exe" : "typst-redline");
  return user || (fs.existsSync(bundled) ? bundled : "typst-redline");
}

function runCli(doc: vscode.TextDocument, args: string[], token?: vscode.CancellationToken) {
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const exe = executable(config);
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

// `what` names the changes in the progress title. A commit base runs from `rev` to `to`, or
// to the file as it is now; `ends` are the revisions where ticked colour steps end.
type Base = { rev: string; to?: string; ends?: string[]; what: string } | { file: string; what: string };
// `parent` is the first parent's short hash: the base that includes the commit's own change.
type Commit = { hash: string; parent: string; date: string; subject: string };
type BaseItem = vscode.QuickPickItem & { base?: Base | "between" | "several" | "other" };

function unsaved(doc: vscode.TextDocument, root: string) {
  return vscode.workspace.textDocuments.filter(
    (d) => d.isDirty && d.uri.scheme === "file" && (d === doc || inside(d.fileName, root)),
  );
}

async function git(dir: string, ...args: string[]) {
  return (await run("git", ["-c", "log.showSignature=false", ...args], dir, "")).stdout;
}

function exe(doc: vscode.TextDocument) {
  return executable(vscode.workspace.getConfiguration("typstRedline", doc.uri));
}

// One list; resolves to the accepted items, or undefined when the list is closed. With
// `view`, the title names the view, or only its layout, and a button switches the layout in place.
function pick<T extends vscode.QuickPickItem>(
  items: readonly T[],
  o: { title: string; placeholder: string; active?: T; many?: boolean; view?: "view" | "layout" },
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
    if (o.view) {
      const showView = () => {
        const v = currentView();
        list.title = `${o.title} (${o.view}: ${o.view === "view" ? viewName(v) : layoutName(v)})`;
        list.buttons = [layoutButton(v)];
      };
      showView();
      list.onDidTriggerButton(async () => {
        await switchLayout();
        showView();
      });
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

// The files the document is made of, or the root folder if the CLI cannot tell.
async function documentFiles(doc: vscode.TextDocument, root: string) {
  try {
    const { stdout } = await run(exe(doc), [doc.fileName, "--list-files", "--root", root], path.dirname(doc.fileName), "");
    const files = stdout.split(/\r?\n/).filter((l) => l.trim());
    return files.length ? files : [root];
  } catch {
    return [root];
  }
}

// The CLI follows the first parent, so every commit listed is one it accepts as a base or
// as the end of a step or of a range. A commit without a parent in the repository, the
// first or the edge of a shallow clone, has no version before its change and is left out.
async function commits(dir: string, files: string[]): Promise<{ list: Commit[]; why: string }> {
  try {
    const log = await git(dir, "log", "--first-parent", "-n", "40", "--date=short", "--format=%h%x09%p%x09%ad%x09%s", "--", ...files);
    const all = log
      .split(/\r?\n/)
      .filter((l) => l.trim())
      .map((l) => {
        const [hash, parents, date, ...subject] = l.split("\t");
        return { hash, parent: parents.split(" ")[0], date, subject: subject.join("\t") };
      });
    const list = all.filter((c) => c.parent);
    const why = list.length ? "" : all.length ? "The only commit here has nothing before it to compare with." : "No commits change this document.";
    return { list, why };
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

// HEAD if the document has changes since it, as the run sees them: in its files that git
// tracks, in the file itself if git does not track it, or unsaved. The run counts other
// untracked files as part of both versions.
async function changedSinceHead(doc: vscode.TextDocument, dir: string, root: string, files: string[]) {
  try {
    const [head, tracked, own] = await Promise.all([
      git(dir, "rev-parse", "--short", "HEAD"),
      git(dir, "--no-optional-locks", "status", "--porcelain", "--untracked-files=no", "--", ...files),
      git(dir, "--no-optional-locks", "status", "--porcelain", "--", doc.fileName),
    ]);
    const edited = unsaved(doc, root).some((d) => files.some((f) => inside(d.fileName, f)));
    return tracked.trim() || own.trim() || edited ? head.trim() : undefined;
  } catch {
    return undefined;
  }
}

const meaning = "A commit stands for its own change and every change after it.";

// With `several`, the list of commits to tick opens first where there is one.
async function pickBase(doc: vscode.TextDocument, proj: Promise<Project>, several: boolean): Promise<Base | undefined> {
  const dir = path.dirname(doc.fileName);
  const files = proj.then((p) => (p.repo ? documentFiles(doc, p.root) : []));
  const [{ root, repo, why: noRepo }, reviewed] = await Promise.all([proj, reviewedCommit(doc)]);
  const targets: BaseItem[] = [];
  let list: Commit[] = [];
  let why = noRepo;
  if (repo) {
    const [log, date, head] = await Promise.all([
      files.then((f) => commits(dir, f)),
      reviewed ? git(dir, "log", "-1", "--date=short", "--format=%ad", reviewed).catch(() => "") : "",
      files.then((f) => changedSinceHead(doc, dir, root, f)),
    ]);
    ({ list, why } = log);
    if (reviewed) {
      targets.push({
        label: "$(bookmark) Last reviewed version",
        description: date.trim(),
        base: { rev: "reviewed", what: "changes since the last reviewed version" },
      });
    }
    if (head) {
      targets.push({ label: "$(edit) Changes since the last commit", base: { rev: head, what: "changes since the last commit" } });
    }
    targets.push(...list.map((c) => ({ ...commitItem(c), base: { rev: c.parent, what: `changes in ${quote(c)} and after` } })));
  }
  if (!targets.length) {
    output.appendLine(why);
    return pickFile(doc);
  }
  targets.push(
    { label: "", kind: vscode.QuickPickItemKind.Separator },
    { label: "$(file) Another file...", description: "compare with a .typ file of your choice", base: "other" },
  );
  if (list.length > 1) {
    targets.push({ label: "$(list-selection) Several commits...", description: "a new colour at each ticked commit", base: "several" });
  }
  if (list.length > 0) {
    targets.push({ label: "$(git-compare) Between two commits...", description: "not up to the file as it is now", base: "between" });
  }
  for (let direct = several && list.length > 1; ; direct = false) {
    const base = direct
      ? "several"
      : (
          await pick(targets, {
            title: "Typst Redline: Show Changes",
            placeholder: meaning,
            active: targets.find((t) => t.kind !== vscode.QuickPickItemKind.Separator),
            view: "view",
          })
        )?.[0]?.base;
    if (base === "several") {
      const picked = await pickSeveral(list);
      if (picked === "back") {
        continue;
      }
      return picked;
    }
    return base === "between" ? pickBetween(list) : base === "other" ? pickFile(doc) : base;
  }
}

async function pickFile(doc: vscode.TextDocument): Promise<Base | undefined> {
  const other = await vscode.window.showOpenDialog({
    title: `Older version of ${path.basename(doc.fileName)}`,
    defaultUri: vscode.Uri.file(path.dirname(doc.fileName)),
    filters: { Typst: ["typ"] },
  });
  return other && { file: other[0].fsPath, what: `changes since ${path.basename(other[0].fsPath)}` };
}

// The most steps the CLI has colours for.
const maxColours = 6;

async function pickSeveral(list: Commit[]): Promise<Base | "back" | undefined> {
  let items = list.map(commitItem);
  for (;;) {
    const ticked = await pick(items, {
      title: "Typst Redline: Several Commits",
      placeholder: `Tick up to ${maxColours} commits; each one starts a new colour`,
      many: true,
      view: "layout",
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
    const [rev, ...ends] = ticked.map((i) => i.c.parent).reverse();
    return { rev, what: `changes in ${quote(ticked[ticked.length - 1].c)} and after`, ends: ends.length ? ends : undefined };
  }
}

async function pickBetween(list: Commit[]): Promise<Base | undefined> {
  const [from] = (await pick(list.map(commitItem), {
    title: "Typst Redline: Between Two Commits",
    placeholder: "Pick the oldest commit to show",
    view: "view",
  })) ?? [undefined];
  if (!from) {
    return undefined;
  }
  const [to] = (await pick(list.slice(0, list.indexOf(from.c) + 1).map(commitItem), {
    title: "Typst Redline: Between Two Commits",
    placeholder: "Pick the newest commit to show",
    view: "view",
  })) ?? [undefined];
  return to && { rev: from.c.parent, to: to.c.hash, what: `changes from ${quote(from.c)} to ${quote(to.c)}` };
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

function layoutName(v: View) {
  return layouts.find((l) => l.layout === v.layout)!.label;
}

function viewName(v: View) {
  const layout = layoutName(v);
  return v.colours === "none"
    ? layout
    : `${layout}, coloured by commit${typeof v.colours === "number" ? `, ${v.colours} steps` : ""}`;
}

// Keeps the colours of the view.
function switchLayout() {
  const v = currentView();
  return state.update("view", { ...v, layout: v.layout === "marked" ? "side-by-side" : "marked" });
}

function layoutButton(v: View): vscode.QuickInputButton {
  const other = layouts.find((l) => l.layout !== v.layout)!.label.toLowerCase();
  return {
    iconPath: new vscode.ThemeIcon(v.layout === "marked" ? "file" : "book"),
    tooltip: `Switch to ${other}`,
  };
}

// Chooses and stores the view; a coloured view also chooses how the commits are coloured.
async function pickView() {
  const now = currentView();
  const items = layouts.flatMap((l) => [
    { label: l.label, detail: l.detail, layout: l.layout, coloured: false },
    { label: `${l.label}, coloured by commit`, detail: `${l.detail}, in colours by commit`, layout: l.layout, coloured: true },
  ]);
  const [view] = (await pick(items, {
    title: "Typst Redline: Choose View",
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
      title: "Typst Redline: Colours",
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
        "Remove it from that setting; choose the view with Typst Redline: Choose View or the button of the list.",
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

// Whether `cmd` can be started; any other failure is left to the run to report.
function found(cmd: string, cwd: string) {
  return run(cmd, ["--version"], cwd, "").then(
    () => true,
    (e: Error) => !e.message.startsWith(`"${cmd}" was not found`),
  );
}

// Says which program is missing, with buttons that help to get it, and whether both are there.
// `again` runs the command again once typst is located.
async function programsFound(file: vscode.Uri, again: () => unknown) {
  const config = vscode.workspace.getConfiguration("typstRedline", file);
  const dir = path.dirname(file.fsPath);
  const tool = executable(config);
  const typstSet = config.get<string>("typst");
  const [toolFound, typstFound] = await Promise.all([found(tool, dir), found(typstSet || "typst", dir)]);
  const restart = "VS Code may need a restart after installing.";
  if (!toolFound) {
    // A bundled copy is always found, so either the setting is wrong or nothing is bundled.
    const choice = tool !== "typst-redline"
      ? vscode.window.showErrorMessage("Typst Redline: the typst-redline program set in the settings was not found.", "Open setting")
      : vscode.window.showErrorMessage(
          `Typst Redline: the typst-redline program was not found. Install it, or set its path in the settings. ${restart}`,
          "How to install",
          "Open setting",
        );
    choice.then((c) =>
      c === "How to install"
        ? vscode.env.openExternal(vscode.Uri.parse("https://github.com/cvsvensson/typst-redline#install"))
        : c && vscode.commands.executeCommand("workbench.action.openSettings", "typstRedline.executable"),
    );
  } else if (!typstFound) {
    const choice = vscode.window.showErrorMessage(
      typstSet
        ? "Typst Redline: the typst set in typstRedline.typst was not found."
        : `Typst Redline: typst was not found. Install it, or set typstRedline.typst to where it is. ${restart}`,
      "Locate typst...",
      "How to install",
    );
    choice.then(async (c) => {
      if (c === "How to install") {
        vscode.env.openExternal(vscode.Uri.parse("https://github.com/typst/typst#installation"));
      } else if (c) {
        const picked = await vscode.window.showOpenDialog({ title: "Locate typst" });
        if (picked) {
          // Where a value is set already, or it would still take precedence.
          const set = config.inspect<string>("typst");
          const target = set?.workspaceFolderValue
            ? vscode.ConfigurationTarget.WorkspaceFolder
            : set?.workspaceValue
              ? vscode.ConfigurationTarget.Workspace
              : vscode.ConfigurationTarget.Global;
          await config.update("typst", picked[0].fsPath, target);
          again();
        }
      }
    });
  }
  return toolFound && typstFound;
}

async function showChanges(clicked?: vscode.Uri, chosen?: Base, several = false) {
  const doc = await typstDocument(clicked);
  const file = doc.fileName;
  const config = vscode.workspace.getConfiguration("typstRedline", doc.uri);
  const extra = config.get<string[]>("extraArgs", []);
  checkExtraArgs(extra);
  // A chosen base comes from a command that checked the programs already.
  const again = () => vscode.commands.executeCommand(several ? "typstRedline.showSeveral" : "typstRedline.showChanges", clicked);
  if (!idle(file) || (!chosen && !(await programsFound(doc.uri, again)))) {
    return;
  }
  const proj = project(doc);
  const base = chosen ?? (await pickBase(doc, proj, several));
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
      { location: vscode.ProgressLocation.Notification, title: `Typst Redline: ${base.what}`, cancellable: true },
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

// VS Code passes the clicked file and all selected ones. The direction is asked, with the
// file changed last offered as the new one.
async function compareFiles(clicked?: vscode.Uri, selected?: vscode.Uri[]) {
  const files = selected?.length ? selected : clicked ? [clicked] : [];
  if (files.length !== 2 || !files.every((f) => f.scheme === "file" && f.fsPath.endsWith(".typ"))) {
    throw new Error("Select two .typ files in the Explorer first.");
  }
  if (!(await programsFound(files[0], () => vscode.commands.executeCommand("typstRedline.compareFiles", clicked, selected)))) {
    return;
  }
  const [a, b] = files.map((f) => f.fsPath);
  const [older, newer] = (await vscode.workspace.fs.stat(files[0])).mtime > (await vscode.workspace.fs.stat(files[1])).mtime ? [b, a] : [a, b];
  const same = path.basename(older) === path.basename(newer);
  const name = (f: string) => (same ? vscode.workspace.asRelativePath(f) : path.basename(f));
  const items = [
    [older, newer],
    [newer, older],
  ].map(([old, now]) => ({ label: `Changes in ${name(now)} since ${name(old)}`, old, now }));
  const [choice] = (await pick(items, { title: "Typst Redline: Show Changes Between These Files", placeholder: "Which file is the newer version?", active: items[0] })) ?? [
    undefined,
  ];
  if (choice) {
    await showChanges(vscode.Uri.file(choice.now), { file: choice.old, what: `changes since ${name(choice.old)}` });
  }
}

async function markReviewed() {
  const doc = await typstDocument();
  const message = await runCli(doc, [doc.fileName, "--mark-reviewed"]);
  vscode.window.showInformationMessage(`Typst Redline: ${message.match(/^marked \S+ as reviewed/)?.[0] ?? message}`);
}

function command(name: string, body: (...args: any[]) => Promise<void>) {
  return vscode.commands.registerCommand(name, async (...args: any[]) => {
    try {
      await body(...args);
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
  extensionDir = context.extensionPath;
  context.subscriptions.push(
    output,
    command("typstRedline.showChanges", (uri?: vscode.Uri) => showChanges(uri)),
    command("typstRedline.compareFiles", compareFiles),
    command("typstRedline.chooseView", pickView),
    command("typstRedline.showSeveral", (uri?: vscode.Uri) => showChanges(uri, undefined, true)),
    command("typstRedline.markReviewed", markReviewed),
  );
}

export function deactivate() {
  running.forEach(killTree);
}
