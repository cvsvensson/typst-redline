import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { execFileSync } from "child_process";
import * as vscode from "vscode";

// A release build of this repository if there is one, otherwise typst-redline on PATH.
const release = path.resolve(
  __dirname,
  "../../../target/release",
  process.platform === "win32" ? "typst-redline.exe" : "typst-redline",
);
const binary = fs.existsSync(release) ? release : "typst-redline";

function git(cwd: string, ...args: string[]) {
  execFileSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.com", ...args], { cwd });
}

type Item = vscode.QuickPickItem;

suite("Typst Redline commands", () => {
  const window = vscode.window as any;
  const commands = vscode.commands as any;
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
  const config = vscode.workspace.getConfiguration("typstRedline");
  const settings = ["executable", "typst", "extraArgs", "openPdf"];
  let tmp: string;
  let file: string;
  let picks: string[];
  let listed: Item[];
  let options: string[];
  let steps: string;
  let otherFile: string;
  let opened: { uri: vscode.Uri; options: any }[];
  let errors: string[];
  let warnings: string[];
  let infos: string[];

  async function open(path: string) {
    file = path;
    await vscode.window.showTextDocument(vscode.Uri.file(file));
  }

  setup(async () => {
    // Inside the open folder, which is then above the repository.
    tmp = fs.mkdtempSync(path.join(vscode.workspace.workspaceFolders![0].uri.fsPath, "t-"));
    const dir = path.join(tmp, "my notes");
    fs.mkdirSync(dir);
    git(dir, "init", "-q");
    const doc = path.join(dir, "nöte.typ");
    fs.writeFileSync(doc, "= Title\n\nThe old sentence.\n");
    git(dir, "add", ".");
    git(dir, "commit", "-q", "-m", "first");
    fs.writeFileSync(doc, "= Title\n\nThe new sentence.\n");
    git(dir, "commit", "-q", "-am", "second");
    await open(doc);

    picks = ["first"];
    listed = [];
    options = [];
    steps = "";
    otherFile = "";
    opened = [];
    errors = [];
    warnings = [];
    infos = [];
    window.showQuickPick = async (items: Item[], opts: vscode.QuickPickOptions) => {
      if (opts.canPickMany) {
        return items.filter((i) => options.includes(i.label));
      }
      listed = items;
      const label = picks.shift();
      return label === undefined ? undefined : items.find((i) => i.label.includes(label));
    };
    window.showInputBox = async () => steps;
    window.showOpenDialog = async () => [vscode.Uri.file(otherFile)];
    window.showErrorMessage = async (message: string) => void errors.push(message);
    window.showWarningMessage = async (message: string) => void warnings.push(message);
    window.showInformationMessage = async (message: string) => void infos.push(message);
    commands.executeCommand = (command: string, ...args: any[]) =>
      command === "vscode.open"
        ? void opened.push({ uri: args[0], options: args[1] })
        : original.executeCommand(command, ...args);
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await config.update("openPdf", "editor", vscode.ConfigurationTarget.Global);
  });

  teardown(async () => {
    // Forget the remembered options: tick none, then leave the list.
    picks = ["Options"];
    options = [];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    Object.assign(window, original);
    commands.executeCommand = original.executeCommand;
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

  test("opens a PDF of the changes since the picked commit beside the editor", async () => {
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    assert.strictEqual(opened[0].options.viewColumn, vscode.ViewColumn.Beside);
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
    picks = ["edit chapter"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
  });

  test("the options side by side, steps and no moves run and are kept for later runs", async () => {
    picks = ["Options", "first"];
    options = ["Side by side", "Colour by step", "No moves"];
    steps = "2";
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    picks = [];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    const shown = listed.find((i) => i.label.includes("Options"));
    assert.strictEqual(shown?.description, "Side by side, Colour by step, No moves");
  });

  test("both colour options are refused", async () => {
    picks = ["Options", "first"];
    options = ["Colour by commit", "Colour by step"];
    steps = "2";
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(warnings.length, 1);
    assertOpenedPdf();
    assert.strictEqual(listed.find((i) => i.label.includes("Options"))?.description, "none");
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

  test("since last reviewed needs a mark, which mark as reviewed sets", async () => {
    picks = ["Last reviewed"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.strictEqual(errors.length, 1);
    assert.match(errors[0], /no commit is marked as reviewed/);
    errors = [];
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.match(infos[0], /marked \w+ as reviewed/);
    picks = ["Last reviewed"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
    // The CLI's hint names its command-line flag; it is not shown as a
    // notification. That nothing changed since the mark is.
    assert.strictEqual(warnings.length, 1);
    assert.match(warnings[0], /no changes found/);
  });

  test("diagnostics of a successful run are shown", async () => {
    fs.writeFileSync(file, "= Title\n\nUncommitted.\n");
    await vscode.commands.executeCommand("typstRedline.markReviewed");
    assert.deepStrictEqual(errors, []);
    assert.strictEqual(warnings.length, 1);
    assert.match(warnings[0], /uncommitted changes/);
  });

  test("compares with another file outside git, without the git-only options", async () => {
    const dir = path.join(tmp, "no git");
    fs.mkdirSync(dir);
    otherFile = path.join(dir, "old.typ");
    fs.writeFileSync(otherFile, "Old text.\n");
    fs.writeFileSync(path.join(dir, "new.typ"), "New text.\n");
    await open(path.join(dir, "new.typ"));
    picks = ["Options", "Another file"];
    options = ["Colour by commit"];
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assertOpenedPdf();
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
    try {
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
    } finally {
      window.withProgress = original.withProgress;
    }
    assert.deepStrictEqual(errors, []);
  });
});
