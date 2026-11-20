import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as childProcess from "child_process";
import * as os from "os";
import * as vscode from "vscode";
import { executable } from "../extension";

// A release build of this repository if there is one, otherwise typst-redline on PATH.
const release = path.resolve(
  __dirname,
  "../../../target/release",
  process.platform === "win32" ? "typst-redline.exe" : "typst-redline",
);
const binary = fs.existsSync(release) ? release : "typst-redline";

function git(cwd: string, ...args: string[]) {
  const config = ["user.name=test", "user.email=test@example.com", "commit.gpgsign=false"];
  return childProcess
    .execFileSync("git", [...config.flatMap((c) => ["-c", c]), ...args], { cwd })
    .toString()
    .trim();
}

type Item = vscode.QuickPickItem;

suite("Typst Redline commands", () => {
  const window = vscode.window as any;
  const commands = vscode.commands as any;
  const cp = childProcess as any;
  const original = {
    createQuickPick: window.createQuickPick,
    showOpenDialog: window.showOpenDialog,
    showErrorMessage: window.showErrorMessage,
    showWarningMessage: window.showWarningMessage,
    showInformationMessage: window.showInformationMessage,
    executeCommand: commands.executeCommand,
    withProgress: window.withProgress,
  };
  const spawn = cp.spawn;
  const config = vscode.workspace.getConfiguration("typstRedline");
  const settings = ["executable", "typst", "extraArgs", "openPdf", "root"];
  let tmp: string;
  let file: string;
  // The labels to pick in the lists shown, in order: "" accepts the active item, a missing
  // one closes the list, and "[button]" switches the layout in the open list.
  let picks: string[];
  // The labels to tick in each list of commits to tick, in order.
  let ticks: string[][];
  let lists: Item[][];
  let actives: (Item | undefined)[];
  let placeholders: string[];
  let titles: string[];
  let otherFile: string;
  let dialogs: string[];
  let opened: { uri: vscode.Uri; options: any }[];
  let runs: string[][];
  let errors: string[];
  let warnings: string[];
  let infos: string[];

  async function open(path: string) {
    file = path;
    await vscode.window.showTextDocument(vscode.Uri.file(file));
  }

  const hash = (subject: string) => git(path.dirname(file), "log", "--format=%h", "-n", "1", `--grep=^${subject}$`);
  const labels = (list: Item[]) => list.map((i) => i.label);
  const show = () => vscode.commands.executeCommand("typstRedline.showChanges");

  async function setView(...choice: string[]) {
    picks = choice;
    await vscode.commands.executeCommand("typstRedline.chooseView");
    lists = [];
    actives = [];
    placeholders = [];
    titles = [];
  }

  function fakeQuickPick() {
    const list: any = {
      items: [],
      activeItems: [],
      selectedItems: [],
      onDidAccept: (f: () => void) => (list.accept = f),
      onDidHide: (f: () => void) => (list.closed = f),
      onDidTriggerButton: (f: (b: vscode.QuickInputButton) => Promise<void>) => (list.button = f),
      hide: () => list.closed?.(),
      dispose: () => {},
      show: () =>
        setTimeout(async () => {
          lists.push(list.items);
          actives.push(list.activeItems[0]);
          placeholders.push(list.placeholder);
          titles.push(list.title);
          for (; picks[0] === "[button]"; titles.push(list.title)) {
            picks.shift();
            await list.button(list.buttons[0]);
          }
          if (list.canSelectMany) {
            const t = ticks.shift();
            list.selectedItems = t ? list.items.filter((i: Item) => t.includes(i.label)) : [];
            return t ? list.accept() : list.hide();
          }
          const label = picks.shift();
          const items: Item[] = list.items;
          const item =
            label === ""
              ? list.activeItems[0]
              : label !== undefined && (items.find((i) => i.label === label) ?? items.find((i) => i.label.includes(label)));
          if (!item) {
            return list.hide();
          }
          list.activeItems = [item];
          list.accept();
        }),
    };
    return list;
  }

  setup(async () => {
    // Inside the open folder, which is then above the repository.
    tmp = fs.mkdtempSync(path.join(vscode.workspace.workspaceFolders![0].uri.fsPath, "t-"));
    const dir = path.join(tmp, "my notes");
    fs.mkdirSync(dir);
    git(dir, "init", "-q");
    // Coloured output must not reach the list of commits.
    git(dir, "config", "color.ui", "always");
    const doc = path.join(dir, "nöte.typ");
    fs.writeFileSync(doc, "= Title\n\nThe old sentence.\n");
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "first");
    fs.writeFileSync(doc, "= Title\n\nThe new sentence.\n");
    git(dir, "commit", "-q", "-am", "second");
    fs.writeFileSync(doc, "= Title\n\nThe newer sentence.\n");
    git(dir, "commit", "-q", "-am", "third – ändrad “här”");
    await open(doc);

    ticks = [];
    lists = [];
    actives = [];
    placeholders = [];
    titles = [];
    otherFile = "";
    dialogs = [];
    opened = [];
    runs = [];
    errors = [];
    warnings = [];
    infos = [];
    window.createQuickPick = fakeQuickPick;
    window.showOpenDialog = async (o: vscode.OpenDialogOptions) => {
      dialogs.push(o.title!);
      return otherFile ? [vscode.Uri.file(otherFile)] : undefined;
    };
    // VS Code's own extensions may show messages in the test instance too.
    const own = (list: string[]) => async (message: string) =>
      void (message.startsWith("Typst Redline") && list.push(message));
    window.showErrorMessage = own(errors);
    window.showWarningMessage = own(warnings);
    window.showInformationMessage = own(infos);
    commands.executeCommand = (command: string, ...args: any[]) =>
      command === "vscode.open"
        ? void opened.push({ uri: args[0], options: args[1] })
        : original.executeCommand(command, ...args);
    cp.spawn = (cmd: string, args: string[], ...rest: any[]) => {
      if (cmd === binary && !args.includes("--show-reviewed") && !args.includes("--list-files") && !args.includes("--version")) {
        runs.push(args);
      }
      return spawn(cmd, args, ...rest);
    };
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await config.update("openPdf", "editor", vscode.ConfigurationTarget.Global);
    await setView("Marked");
    picks = [""];
  });

  teardown(async () => {
    Object.assign(window, original);
    commands.executeCommand = original.executeCommand;
    cp.spawn = spawn;
    for (const s of settings) {
      await config.update(s, undefined, vscode.ConfigurationTarget.Global);
    }
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    // On Windows the test instance holds the folder open, so removing it throws EPERM
    // although the folder does disappear once the instance exits.
    try {
      fs.rmSync(tmp, { recursive: true, force: true });
    } catch {}
  });

  function assertOpenedPdf() {
    assert.deepStrictEqual(errors, []);
    assert.strictEqual(opened.length, 1);
    assert.strictEqual(fs.readFileSync(opened[0].uri.fsPath).subarray(0, 5).toString(), "%PDF-");
  }

  test("command titles are plain text", () => {
    const ext = vscode.extensions.all.find((e) => e.packageJSON.name === "typst-redline")!;
    for (const c of ext.packageJSON.contributes.commands) {
      assert.match(c.title, /^[\x20-\x7e]+$/);
    }
  });

  test("lists commits that have a parent by their subject as written, then the other choices", async () => {
    picks = [];
    await show();
    const words = labels(lists[0]).map((l) => l.replace(/^\$\([^)]*\) /, ""));
    assert.deepStrictEqual(words, ["third – ändrad “här”", "second", "", "Another file...", "Several commits...", "Between two commits..."]);
  });

  test("Enter shows the newest commit's change, and the PDF opens beside the editor", async () => {
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.strictEqual(lists.length, 1);
    assert.strictEqual(opened[0].options.viewColumn, vscode.ViewColumn.Beside);
    assert.deepStrictEqual(runs[0].slice(0, 4), [file, "--since", hash("second"), "--root"]);
    assert.strictEqual(runs[0].length, 5);
  });

  test("a file passed by a menu is compared, not the active editor", async () => {
    const dir = path.dirname(file);
    const other = path.join(dir, "other.typ");
    fs.writeFileSync(other, "Old other.\n");
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "add other");
    fs.writeFileSync(other, "New other.\n");
    picks = ["add other"];
    await vscode.commands.executeCommand("typstRedline.showChanges", vscode.Uri.file(other));
    assertOpenedPdf();
    assert.strictEqual(vscode.window.activeTextEditor?.document.fileName, file);
    assert.deepStrictEqual(runs[0].slice(0, 3), [vscode.Uri.file(other).fsPath, "--since", hash("third.*")]);
  });

  test("the first list names the view, and its button switches the layout in place for this and later runs", async () => {
    await setView("Side by side, coloured by commit", "4 steps");
    picks = ["[button]", "second"];
    await show();
    assert.deepStrictEqual(titles, [
      "Typst Redline: Show Changes (view: Side by side, coloured by commit, 4 steps)",
      "Typst Redline: Show Changes (view: Marked, coloured by commit, 4 steps)",
    ]);
    assert.strictEqual(lists.length, 1);
    assert.strictEqual(actives[0]?.label, "third – ändrad “här”");
    assert.deepStrictEqual(runs[0].slice(5), ["--steps", "4"]);
    picks = [];
    await show();
    assert.strictEqual(titles[2], titles[1]);
  });



  test("each view passes its options", async () => {
    const expected: [string[], string[]][] = [
      [["Side by side, coloured by commit", "One colour per commit"], ["--layout", "side-by-side", "--each-commit"]],
      [["Side by side"], ["--layout", "side-by-side"]],
      [["Marked, coloured by commit", "2 steps"], ["--steps", "2"]],
      [["Marked"], []],
    ];
    for (const [view, args] of expected) {
      await setView(...view);
      picks = ["second"];
      opened = [];
      runs = [];
      await show();
      assertOpenedPdf();
      assert.deepStrictEqual(runs[0].slice(5), args, view.join(" "));
    }
  });

  test("more commits than colours are coloured in six steps", async () => {
    const dir = path.dirname(file);
    for (let i = 0; i < 6; i++) {
      fs.appendFileSync(file, `\nLine ${i}.\n`);
      git(dir, "commit", "-q", "-am", `line ${i}`);
    }
    await setView("Marked, coloured by commit", "One colour per commit");
    picks = ["second"];
    await show();
    assertOpenedPdf();
    assert.deepStrictEqual(runs[1].slice(5), ["--steps", "6"]);
    assert.match(infos[0], /more than 6 colours/);
  });

  test("changes between two commits include both and nothing after", async () => {
    await setView("Marked, coloured by commit", "One colour per commit");
    picks = ["Between two commits", "second", "third"];
    await show();
    assertOpenedPdf();
    assert.deepStrictEqual(labels(lists[2]), ["third – ändrad “här”", "second"]);
    const args = [file, "--since", hash("first"), "--to", hash("third.*"), "--root"];
    assert.deepStrictEqual(runs[0].slice(0, 6), args);
    assert.deepStrictEqual(runs[0].slice(7), ["--each-commit"]);
    // The summary names the commits it colours.
    const kept = childProcess.spawnSync(binary, [...runs[0], "--keep-typ", "-o", path.join(tmp, "k.pdf")]);
    const dir = kept.stderr.toString().match(/^kept (.*)$/m)![1].trim();
    const src = fs.readFileSync(path.join(dir, path.basename(file)), "utf8");
    fs.rmSync(dir, { recursive: true, force: true });
    const subjects = [...src.matchAll(/subject: "([^"]*)"/g)].map((m) => m[1]);
    assert.deepStrictEqual(subjects.sort(), ["second", "third – ändrad “här”"]);
  });

  test("ticked commits start a colour each, their own change included, in the view's layout", async () => {
    await setView("Side by side");
    picks = ["Several commits"];
    ticks = [["third – ändrad “här”", "second"]];
    await show();
    assertOpenedPdf();
    assert.strictEqual(titles[1], "Typst Redline: Several Commits (layout: Side by side)");
    assert.deepStrictEqual(runs[0].slice(1, 3), ["--since", hash("first")]);
    assert.deepStrictEqual(runs[0].slice(5), ["--layout", "side-by-side", "--steps", hash("second")]);
  });

  test("the command for several commits opens the list to tick, and ticks pass the same arguments", async () => {
    picks = ["Several commits"];
    ticks = [["third – ändrad “här”", "second"]];
    await show();
    lists = [];
    picks = [];
    ticks = [["third – ändrad “här”", "second"]];
    await vscode.commands.executeCommand("typstRedline.showSeveral");
    assert.strictEqual(lists.length, 1);
    assert.strictEqual(runs.length, 2);
    assert.deepStrictEqual(runs[1], runs[0]);
  });

  test("too many ticks are refused and none goes back", async () => {
    const dir = path.dirname(file);
    for (let i = 0; i < 6; i++) {
      fs.appendFileSync(file, `\nLine ${i}.\n`);
      git(dir, "commit", "-q", "-am", `line ${i}`);
    }
    const seven = ["line 0", "line 1", "line 2", "line 3", "line 4", "line 5", "second"];
    picks = ["Several commits"];
    ticks = [seven, []];
    await show();
    assert.strictEqual(warnings.length, 1);
    assert.strictEqual(lists[2].filter((i) => i.picked).length, 7);
    // No tick returns to the first list.
    assert.deepStrictEqual(labels(lists[3]), labels(lists[0]));
    assert.deepStrictEqual(runs, []);
  });

  test("a merge lists the commits of the first parent, which can be ticked", async () => {
    const dir = path.dirname(file);
    git(dir, "checkout", "-q", "-b", "side");
    fs.appendFileSync(file, "\nSide.\n");
    git(dir, "commit", "-q", "-am", "side work");
    git(dir, "checkout", "-q", "-");
    git(dir, "merge", "-q", "--no-ff", "side", "-m", "merge side");
    picks = ["Several commits"];
    ticks = [["merge side", "second"]];
    await show();
    assert.ok(!labels(lists[0]).includes("side work"));
    assertOpenedPdf();
  });

  test("extra arguments that the command sets itself are refused", async () => {
    await config.update("extraArgs", ["--steps", "3"], vscode.ConfigurationTarget.Global);
    await show();
    assert.deepStrictEqual(runs, []);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /typstRedline\.extraArgs holds --steps.*Choose View/);
  });

  test("with nothing to colour, the changes are marked without colours and it says so", async () => {
    const dir = path.dirname(file);
    fs.writeFileSync(path.join(dir, "new.typ"), "New.\n");
    await open(path.join(dir, "new.typ"));
    await setView("Marked, coloured by commit", "One colour per commit");
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.deepStrictEqual(infos, [
      'Typst Redline: no commit in this range changes this document, so the changes are not coloured.',
    ]);
    assert.strictEqual(runs[1].length, 5);
  });

  test("with no commits to list, the dialog for the older version opens at once", async () => {
    const empty = path.join(tmp, "empty");
    fs.mkdirSync(empty);
    git(empty, "init", "-q");
    fs.writeFileSync(path.join(empty, "a.typ"), "A.\n");
    await open(path.join(empty, "a.typ"));
    await show();
    assert.deepStrictEqual(lists, []);
    assert.deepStrictEqual(dialogs, ["Older version of a.typ"]);
  });

  test("two files selected in the Explorer: the direction is asked, the file changed last offered as new", async () => {
    const dir = path.dirname(file);
    const [a, b] = ["a.typ", "b.typ"].map((n) => path.join(dir, n));
    fs.writeFileSync(a, "Old.\n");
    fs.writeFileSync(b, "New.\n");
    fs.utimesSync(a, new Date(2020, 0, 1), new Date(2020, 0, 1));
    const uris = [a, b].map((f) => vscode.Uri.file(f));
    picks = [""];
    await vscode.commands.executeCommand("typstRedline.compareFiles", uris[0], uris);
    assertOpenedPdf();
    assert.deepStrictEqual(labels(lists[0]), ["Changes in b.typ since a.typ", "Changes in a.typ since b.typ"]);
    assert.deepStrictEqual(runs[0].slice(0, 2), uris.map((u) => u.fsPath));
  });

  test("changes since the last commit are listed first only when the document's files changed", async () => {
    const dir = path.dirname(file);
    fs.writeFileSync(path.join(dir, "notes.txt"), "Notes.\n");
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "add notes");
    fs.writeFileSync(path.join(dir, "notes.txt"), "Other notes.\n");
    fs.writeFileSync(path.join(dir, "untracked.pdf"), "");
    picks = [];
    await show();
    // Neither the commit nor the change of a file the document does not use is listed.
    assert.strictEqual(actives[0]?.label, "third – ändrad “här”");
    assert.ok(!labels(lists[0]).some((l) => /add notes|last commit/.test(l)));
    fs.writeFileSync(file, "= Title\n\nUncommitted.\n");
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.match(actives[1]!.label, /Changes since the last commit/);
    assert.deepStrictEqual(runs[0].slice(1, 3), ["--since", hash("add notes")]);
  });

  test("unsaved changes are saved and compared", async () => {
    const editor = vscode.window.activeTextEditor!;
    await editor.edit((e) => e.insert(new vscode.Position(0, 0), "Unsaved.\n\n"));
    await show();
    assertOpenedPdf();
    assert.match(fs.readFileSync(file, "utf8"), /^Unsaved\./);
  });

  test("lists commits that changed only an included file", async () => {
    const dir = path.dirname(file);
    fs.writeFileSync(path.join(dir, "chapter.typ"), "Old chapter.\n");
    fs.appendFileSync(file, '#include "chapter.typ"\n');
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "add chapter");
    fs.writeFileSync(path.join(dir, "chapter.typ"), "New chapter.\n");
    git(dir, "commit", "-q", "-am", "edit chapter");
    picks = ["edit chapter"];
    await show();
    assertOpenedPdf();
  });

  test("a usage error shows the CLI's message", async () => {
    await config.update("extraArgs", ["--nosuchoption"], vscode.ConfigurationTarget.Global);
    await show();
    assert.strictEqual(opened.length, 0);
    assert.deepStrictEqual(errors, ["Typst Redline: unknown option --nosuchoption"]);
  });

  test("a compile error shows typst's message", async () => {
    fs.writeFileSync(file, "= Title\n\nThe #nosuchvariable sentence.\n");
    await show();
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /unknown variable: nosuchvariable/);
  });

  test("extra arguments reach the CLI and typst", async () => {
    const out = path.join(tmp, "out.pdf");
    fs.writeFileSync(file, "= Title\n\nThe new sentence by #sys.inputs.who.\n");
    await config.update("extraArgs", ["-o", out, "--", "--input", "who=me"], vscode.ConfigurationTarget.Global);
    await show();
    assertOpenedPdf();
    assert.strictEqual(opened[0].uri.fsPath, vscode.Uri.file(out).fsPath);
  });

  test("the last reviewed version is listed first once mark as reviewed set it", async () => {
    picks = [];
    await show();
    assert.ok(!labels(lists[0]).some((l) => l.includes("Last reviewed")));
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.deepStrictEqual(infos, [`Typst Redline: marked ${hash("third.*")} as reviewed`]);
    fs.writeFileSync(file, "= Title\n\nAfter the review.\n");
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.match(actives[1]!.label, /Last reviewed/);
    assert.deepStrictEqual(runs[1].slice(1, 3), ["--since", "reviewed"]);
  });

  test("diagnostics of a successful run are shown", async () => {
    fs.writeFileSync(file, "= Title\n\nUncommitted.\n");
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.deepStrictEqual(errors, []);
    assert.strictEqual(warnings.length, 1);
    assert.match(warnings[0], /uncommitted changes/);
  });

  test("compares with another file outside git, without colours", async () => {
    const dir = path.join(tmp, "no git");
    fs.mkdirSync(dir);
    otherFile = path.join(dir, "old.typ");
    fs.writeFileSync(otherFile, "Old text.\n");
    fs.writeFileSync(path.join(dir, "new.typ"), "New text.\n");
    await open(path.join(dir, "new.typ"));
    await setView("Side by side, coloured by commit", "One colour per commit");
    await show();
    assertOpenedPdf();
    assert.deepStrictEqual(lists, []);
    assert.deepStrictEqual(runs[0].slice(0, 2), [otherFile, file]);
    assert.deepStrictEqual(runs[0].slice(4), ["--layout", "side-by-side"]);
    assert.strictEqual(infos.length, 1);
  });

  test("cancelling at any step runs nothing", async () => {
    const steps = [
      [],
      ["[button]"],
      ["Several commits"],
      ["Between two commits"],
      ["Between two commits", "second"],
      ["Another file"],
    ];
    for (const p of steps) {
      picks = [...p];
      await show();
    }
    assert.deepStrictEqual(runs, []);
    assert.deepStrictEqual(errors, []);
  });

  test("a missing typst is reported before any list, naming its setting", async () => {
    await config.update("typst", "no-such-typst", vscode.ConfigurationTarget.Global);
    await show();
    const other = vscode.Uri.file(path.join(path.dirname(file), "other.typ"));
    fs.writeFileSync(other.fsPath, "Other.\n");
    await vscode.commands.executeCommand("typstRedline.compareFiles", other, [other, vscode.Uri.file(file)]);
    assert.deepStrictEqual(lists, []);
    assert.deepStrictEqual(runs, []);
    assert.strictEqual(errors.length, 2);
    assert.match(errors[0], /typst .*typstRedline\.typst.*not found/);
  });

  test("a missing executable is reported before any list", async () => {
    await config.update("executable", "no-such-typst-redline", vscode.ConfigurationTarget.Global);
    await show();
    assert.deepStrictEqual(lists, []);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /typst-redline .*not found/);
  });

  test("a second run is refused while one runs, and cancelling ends typst, which the CLI runs as a child", async () => {
    const win = process.platform === "win32";
    const tick = path.join(tmp, "tick");
    const fake = path.join(tmp, win ? "fake-typst.cmd" : "fake-typst");
    fs.writeFileSync(
      fake,
      win
        ? ["@echo off", 'if "%1"=="--version" exit /b 0', ":l", `echo x>>"${tick}"`, "ping -n 2 127.0.0.1 >nul", "goto l", ""].join("\r\n")
        : `#!/bin/sh\n[ "$1" = --version ] && exit 0\nwhile true; do echo x >> "${tick}"; sleep 0.2; done\n`,
      { mode: 0o755 },
    );
    await config.update("typst", fake, vscode.ConfigurationTarget.Global);
    const cancel = new vscode.CancellationTokenSource();
    window.withProgress = (_: unknown, task: Function) => task(undefined, cancel.token);
    const size = () => (fs.existsSync(tick) ? fs.statSync(tick).size : 0);
    const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
    const run = show();
    for (let i = 0; i < 100 && size() === 0; i++) {
      await sleep(100);
    }
    assert.ok(size() > 0, "typst did not start");
    picks = [""];
    await show();
    assert.deepStrictEqual(warnings, ["Typst Redline is still running for this file."]);
    assert.strictEqual(runs.length, 1);
    cancel.cancel();
    await run;
    await sleep(1000);
    const after = size();
    await sleep(1500);
    assert.strictEqual(size(), after);
    assert.deepStrictEqual(errors, []);
  });
});

suite("Typst Redline executable lookup", () => {
  const config = () => vscode.workspace.getConfiguration("typstRedline");
  const name = process.platform === "win32" ? "typst-redline.exe" : "typst-redline";
  let dir: string;
  setup(() => {
    dir = fs.mkdtempSync(path.join(os.tmpdir(), "typst-redline-ext-"));
  });
  teardown(() => config().update("executable", undefined, vscode.ConfigurationTarget.Global));

  test("setting, then bundled binary, then PATH", async () => {
    assert.strictEqual(executable(config(), dir), "typst-redline");
    fs.mkdirSync(path.join(dir, "bin"));
    fs.writeFileSync(path.join(dir, "bin", name), "");
    assert.strictEqual(executable(config(), dir), path.join(dir, "bin", name));
    await config().update("executable", "/my/typst-redline", vscode.ConfigurationTarget.Global);
    assert.strictEqual(executable(config(), dir), "/my/typst-redline");
  });
});
