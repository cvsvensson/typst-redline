import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as childProcess from "child_process";
import * as vscode from "vscode";

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
  // The labels to pick in the lists shown, in order: "" accepts the active item, and a
  // missing one closes the list.
  let picks: string[];
  // The labels to tick in each list of commits to tick, in order.
  let ticks: string[][];
  let lists: Item[][];
  let actives: (Item | undefined)[];
  let placeholders: string[];
  let otherFile: string;
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

  // Chooses the view with the View entry, then leaves the list.
  async function setView(...choice: string[]) {
    picks = ["View", ...choice];
    await show();
    lists = [];
    actives = [];
    placeholders = [];
  }

  function fakeQuickPick() {
    const list: any = {
      items: [],
      activeItems: [],
      selectedItems: [],
      onDidAccept: (f: () => void) => (list.accept = f),
      onDidHide: (f: () => void) => (list.closed = f),
      hide: () => list.closed?.(),
      dispose: () => {},
      show: () =>
        setTimeout(() => {
          lists.push(list.items);
          actives.push(list.activeItems[0]);
          placeholders.push(list.placeholder);
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
    otherFile = "";
    opened = [];
    runs = [];
    errors = [];
    warnings = [];
    infos = [];
    window.createQuickPick = fakeQuickPick;
    window.showOpenDialog = async () => (otherFile ? [vscode.Uri.file(otherFile)] : undefined);
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
      if (cmd === binary && !args.includes("--show-reviewed")) {
        runs.push(args);
      }
      return spawn(cmd, args, ...rest);
    };
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await config.update("openPdf", "editor", vscode.ConfigurationTarget.Global);
    await setView("Marked");
    picks = ["first"];
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

  test("lists the view, then commits by their subject as written", async () => {
    picks = [];
    await show();
    assert.deepStrictEqual(labels(lists[0]).slice(0, 5), [
      "$(settings) View: Marked",
      "",
      "third – ändrad “här”",
      "second",
      "first",
    ]);
  });

  test("Enter runs at once with the newest commit, and the PDF opens beside the editor", async () => {
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.strictEqual(lists.length, 1);
    assert.strictEqual(opened[0].options.viewColumn, vscode.ViewColumn.Beside);
    assert.deepStrictEqual(runs[0].slice(0, 4), [file, "--since", hash("third.*"), "--root"]);
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
    assert.deepStrictEqual(runs[0].slice(0, 3), [vscode.Uri.file(other).fsPath, "--since", hash("add other")]);
  });

  test("the view entry changes the view, which later runs use", async () => {
    await setView("Side by side, coloured by commit", "4 steps");
    picks = [];
    await show();
    assert.strictEqual(lists[0][0].label, "$(settings) View: Side by side, coloured by commit, 4 steps");
    assert.strictEqual(actives[0]?.label, "third – ändrad “här”");
    assert.deepStrictEqual(runs, []);
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
      picks = ["first"];
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
    picks = ["first"];
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
    // The first commit has no parent to compare with.
    assert.deepStrictEqual(labels(lists[1]), ["third – ändrad “här”", "second"]);
    assert.deepStrictEqual(runs[0].slice(1, 3), ["--since", hash("first")]);
    assert.deepStrictEqual(runs[0].slice(5), ["--layout", "side-by-side", "--steps", hash("second")]);
  });

  test("too many ticks are refused and none goes back", async () => {
    const dir = path.dirname(file);
    for (let i = 0; i < 6; i++) {
      fs.appendFileSync(file, `\nLine ${i}.\n`);
      git(dir, "commit", "-q", "-am", `line ${i}`);
    }
    const seven = ["line 0", "line 1", "line 2", "line 3", "line 4", "line 5", "second"];
    picks = ["Several commits", "Several commits"];
    ticks = [seven, [], []];
    await show();
    assert.strictEqual(warnings.length, 1);
    assert.strictEqual(lists[2].filter((i) => i.picked).length, 7);
    // No tick returns to the first list.
    assert.deepStrictEqual(labels(lists[3]), labels(lists[0]));
    assert.deepStrictEqual(runs, []);
  });

  test("a merge lists the commits of the first parent, which can be ticked", async () => {
    const dir = path.dirname(file);
    git(dir, "checkout", "-q", "-b", "side", "HEAD~1");
    fs.writeFileSync(path.join(dir, "side.txt"), "side\n");
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "side work");
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
    assert.match(errors[0], /typstRedline\.extraArgs holds --steps.*View entry/);
  });

  test("with nothing to colour, the changes are marked without colours and it says so", async () => {
    const dir = path.dirname(file);
    fs.writeFileSync(path.join(dir, "new.typ"), "New.\n");
    await open(path.join(dir, "new.typ"));
    await setView("Marked, coloured by commit", "One colour per commit");
    picks = ["first"];
    await show();
    assertOpenedPdf();
    assert.deepStrictEqual(infos, [
      'Typst Redline: no commit in this range changes this document, so the changes are not coloured.',
    ]);
    assert.strictEqual(runs[1].length, 5);
  });

  test("the list says when there are no commits", async () => {
    const empty = path.join(tmp, "empty");
    fs.mkdirSync(path.join(empty, "sub"), { recursive: true });
    git(empty, "init", "-q");
    fs.writeFileSync(path.join(empty, "a.typ"), "A.\n");
    await open(path.join(empty, "a.typ"));
    picks = [];
    await show();
    assert.match(placeholders[0], /^No commits yet\. /);
    git(empty, "add", ".");
    git(empty, "commit", "-q", "-m", "a");
    await config.update("root", path.join(empty, "sub"), vscode.ConfigurationTarget.Global);
    await show();
    assert.match(placeholders[1], /^No commits touch this folder\. /);
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
    picks = [""];
    await show();
    assertOpenedPdf();
    assert.strictEqual(actives[0]?.label, "$(file) Another file...");
    assert.deepStrictEqual(runs[0].slice(0, 2), [otherFile, file]);
    assert.deepStrictEqual(runs[0].slice(4), ["--layout", "side-by-side"]);
    assert.strictEqual(infos.length, 1);
  });

  test("cancelling at any step runs nothing", async () => {
    const steps = [
      [],
      ["View"],
      ["View", "Marked, coloured by commit"],
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

  test("the typst setting is passed to the CLI", async () => {
    await config.update("typst", "no-such-typst", vscode.ConfigurationTarget.Global);
    await show();
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /no-such-typst/);
  });

  test("a missing executable is reported with a hint at the setting", async () => {
    await config.update("executable", "no-such-typst-redline", vscode.ConfigurationTarget.Global);
    await show();
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /not found.*typstRedline\.executable/);
  });

  test("a second run is refused while one runs, and cancelling ends typst, which the CLI runs as a child", async () => {
    const win = process.platform === "win32";
    const tick = path.join(tmp, "tick");
    const fake = path.join(tmp, win ? "fake-typst.cmd" : "fake-typst");
    fs.writeFileSync(
      fake,
      win
        ? ["@echo off", ":l", `echo x>>"${tick}"`, "ping -n 2 127.0.0.1 >nul", "goto l", ""].join("\r\n")
        : `#!/bin/sh\nwhile true; do echo x >> "${tick}"; sleep 0.2; done\n`,
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
    picks = ["first"];
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
