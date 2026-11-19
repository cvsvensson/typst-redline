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
  return childProcess
    .execFileSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.com", ...args], { cwd })
    .toString()
    .trim();
}

type Item = vscode.QuickPickItem;

suite("Typst Redline commands", () => {
  const window = vscode.window as any;
  const commands = vscode.commands as any;
  const cp = childProcess as any;
  const original = {
    showQuickPick: window.showQuickPick,
    showInputBox: window.showInputBox,
    showOpenDialog: window.showOpenDialog,
    showErrorMessage: window.showErrorMessage,
    showWarningMessage: window.showWarningMessage,
    showInformationMessage: window.showInformationMessage,
    executeCommand: commands.executeCommand,
    withProgress: window.withProgress,
  };
  const spawn = cp.spawn;
  const config = vscode.workspace.getConfiguration("typstRedline");
  const settings = ["executable", "typst", "extraArgs", "openPdf", "view"];
  let tmp: string;
  let file: string;
  // The labels to pick in the lists shown, in order; a missing one cancels.
  let picks: string[];
  let ticks: string[];
  let lists: Item[][];
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

    picks = ["first", "Marked PDF"];
    ticks = [];
    lists = [];
    otherFile = "";
    opened = [];
    runs = [];
    errors = [];
    warnings = [];
    infos = [];
    window.showQuickPick = async (items: Item[], opts: vscode.QuickPickOptions) => {
      lists.push(items);
      if (opts.canPickMany) {
        return items.filter((i) => ticks.includes(i.label));
      }
      const label = picks.shift();
      return label === undefined
        ? undefined
        : (items.find((i) => i.label === label) ?? items.find((i) => i.label.includes(label)));
    };
    window.showOpenDialog = async () => (otherFile ? [vscode.Uri.file(otherFile)] : undefined);
    // VS Code's own extensions may show messages in the test instance too.
    const own = (list: string[]) => async (message: string) =>
      void (message.startsWith("Typst Redline:") && list.push(message));
    window.showErrorMessage = own(errors);
    window.showWarningMessage = own(warnings);
    window.showInformationMessage = own(infos);
    commands.executeCommand = (command: string, ...args: any[]) =>
      command === "vscode.open"
        ? void opened.push({ uri: args[0], options: args[1] })
        : original.executeCommand(command, ...args);
    cp.spawn = (cmd: string, args: string[], ...rest: any[]) => {
      if (cmd === binary) {
        runs.push(args);
      }
      return spawn(cmd, args, ...rest);
    };
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await config.update("openPdf", "editor", vscode.ConfigurationTarget.Global);
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

  test("lists commits by their subject as written", async () => {
    picks = [];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.deepStrictEqual(labels(lists[0]).slice(0, 3), ["third – ändrad “här”", "second", "first"]);
  });

  test("opens a PDF of the changes since the picked commit beside the editor", async () => {
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.strictEqual(opened[0].options.viewColumn, vscode.ViewColumn.Beside);
    assert.deepStrictEqual(runs[0].slice(0, 4), [file, "--since", hash("first"), "--root"]);
    assert.strictEqual(runs[0].length, 5);
  });

  test("each view passes its options", async () => {
    const expected: [string, string[]][] = [
      ["Side by side, coloured by commit", ["--layout", "side-by-side", "--each-commit"]],
      ["Side by side", ["--layout", "side-by-side"]],
      ["Coloured by commit", ["--each-commit"]],
      ["Marked PDF", []],
    ];
    for (const [view, args] of expected) {
      picks = ["first", view];
      opened = [];
      runs = [];
      await vscode.commands.executeCommand("typstRedline.showChanges");
      assertOpenedPdf();
      assert.deepStrictEqual(runs[0].slice(5), args, view);
    }
  });

  test("the last used view is offered first", async () => {
    picks = ["first", "Side by side"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    picks = ["first"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(lists[3][0].label, "Side by side");
  });

  test("the view setting skips the choice of view", async () => {
    await config.update("view", "side-by-side-by-commit", vscode.ConfigurationTarget.Global);
    picks = ["first"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.strictEqual(lists.length, 1);
    assert.deepStrictEqual(runs[0].slice(5), ["--layout", "side-by-side", "--each-commit"]);
  });

  test("the views coloured by commit need two commits or changes", async () => {
    picks = ["second"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.deepStrictEqual(labels(lists[1]).sort(), ["Marked PDF", "Side by side"]);
    fs.writeFileSync(file, "= Title\n\nUncommitted.\n");
    picks = ["second"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(lists[3].length, 4);
  });

  test("more commits than colours are grouped", async () => {
    const dir = path.dirname(file);
    for (let i = 0; i < 6; i++) {
      fs.appendFileSync(file, `\nLine ${i}.\n`);
      git(dir, "commit", "-q", "-am", `line ${i}`);
    }
    picks = ["first", "Coloured by commit"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.deepStrictEqual(runs[0].slice(5), ["--steps", "6"]);
  });

  test("ticked commits start a colour each", async () => {
    picks = ["Several commits", "Coloured by commit"];
    ticks = ["third – ändrad “här”", "first"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.deepStrictEqual(labels(lists[2]).sort(), ["Coloured by commit", "Side by side, coloured by commit"]);
    assert.deepStrictEqual(runs[0].slice(1, 3), ["--since", hash("first")]);
    assert.deepStrictEqual(runs[0].slice(5), ["--steps", hash("third.*")]);
  });

  test("cancelling at any step runs nothing", async () => {
    for (const p of [[], ["first"], ["Several commits"], ["Another file"]]) {
      picks = [...p];
      await vscode.commands.executeCommand("typstRedline.showChanges");
    }
    assert.deepStrictEqual(runs, []);
    assert.deepStrictEqual(errors, []);
  });

  test("unsaved changes are saved and compared", async () => {
    const editor = vscode.window.activeTextEditor!;
    await editor.edit((e) => e.insert(new vscode.Position(0, 0), "Unsaved.\n\n"));
    await vscode.commands.executeCommand("typstRedline.showChanges");
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
    picks = ["edit chapter", "Marked PDF"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
  });

  test("a usage error shows the CLI's message", async () => {
    await config.update("extraArgs", ["--nosuchoption"], vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(opened.length, 0);
    assert.deepStrictEqual(errors, ["Typst Redline: unknown option --nosuchoption"]);
  });

  test("a compile error shows typst's message", async () => {
    fs.writeFileSync(file, "= Title\n\nThe #nosuchvariable sentence.\n");
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /unknown variable: nosuchvariable/);
  });

  test("extra arguments reach the CLI and typst", async () => {
    const out = path.join(tmp, "out.pdf");
    fs.writeFileSync(file, "= Title\n\nThe new sentence by #sys.inputs.who.\n");
    await config.update("extraArgs", ["-o", out, "--", "--input", "who=me"], vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.strictEqual(opened[0].uri.fsPath, vscode.Uri.file(out).fsPath);
  });

  test("the last reviewed version is offered first once mark as reviewed set it", async () => {
    picks = [];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.ok(!labels(lists[0]).some((l) => l.includes("Last reviewed")));
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.match(infos[0], /marked \w+ as reviewed/);
    fs.writeFileSync(file, "= Title\n\nAfter the review.\n");
    picks = ["Last reviewed", "Marked PDF"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.match(lists[1][0].label, /Last reviewed/);
    assert.deepStrictEqual(runs[1].slice(1, 3), ["--since", "reviewed"]);
  });

  test("diagnostics of a successful run are shown", async () => {
    fs.writeFileSync(file, "= Title\n\nUncommitted.\n");
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.deepStrictEqual(errors, []);
    assert.strictEqual(warnings.length, 1);
    assert.match(warnings[0], /uncommitted changes/);
  });

  test("compares with another file outside git, in the views it allows", async () => {
    const dir = path.join(tmp, "no git");
    fs.mkdirSync(dir);
    otherFile = path.join(dir, "old.typ");
    fs.writeFileSync(otherFile, "Old text.\n");
    fs.writeFileSync(path.join(dir, "new.typ"), "New text.\n");
    await open(path.join(dir, "new.typ"));
    picks = ["Another file", "Marked PDF"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.deepStrictEqual(labels(lists[1]).sort(), ["Marked PDF", "Side by side"]);
    assert.deepStrictEqual(runs[0].slice(0, 2), [otherFile, file]);
    // A view coloured by commit set in the settings gives the same layout without colours.
    await config.update("view", "by-commit", vscode.ConfigurationTarget.Global);
    picks = ["Another file"];
    opened = [];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.deepStrictEqual(runs[1], runs[0]);
  });

  test("the typst setting is passed to the CLI", async () => {
    await config.update("typst", "no-such-typst", vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /no-such-typst/);
  });

  test("a missing executable is reported with a hint at the setting", async () => {
    await config.update("executable", "no-such-typst-redline", vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(opened.length, 0);
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /not found.*typstRedline\.executable/);
  });

  test("cancelling ends typst, which the CLI runs as a child", async () => {
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
    const run = vscode.commands.executeCommand("typstRedline.showChanges");
    for (let i = 0; i < 100 && size() === 0; i++) {
      await sleep(100);
    }
    assert.ok(size() > 0, "typst did not start");
    cancel.cancel();
    await run;
    await sleep(1000);
    const after = size();
    await sleep(1500);
    assert.strictEqual(size(), after);
    assert.deepStrictEqual(errors, []);
  });
});
