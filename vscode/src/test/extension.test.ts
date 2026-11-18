import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { execFileSync } from "child_process";
import * as vscode from "vscode";

const binary = path.resolve(
  __dirname,
  "../../../target/release",
  process.platform === "win32" ? "typst-redline.exe" : "typst-redline",
);

function git(cwd: string, ...args: string[]) {
  execFileSync("git", ["-c", "user.name=test", "-c", "user.email=test@example.com", ...args], { cwd });
}

suite("Show changes since…", () => {
  const original = {
    showQuickPick: vscode.window.showQuickPick,
    showErrorMessage: vscode.window.showErrorMessage,
    executeCommand: vscode.commands.executeCommand,
  };
  const config = vscode.workspace.getConfiguration("typstRedline");
  let repo: string;
  let opened: vscode.Uri[];
  let errors: string[];

  setup(async () => {
    repo = fs.mkdtempSync(path.join(os.tmpdir(), "typst-redline-"));
    const dir = path.join(repo, "my notes");
    const file = path.join(dir, "nöte.typ");
    fs.mkdirSync(dir);
    git(repo, "init", "-q");
    fs.writeFileSync(file, "= Title\n\nThe old sentence.\n");
    git(repo, "add", ".");
    git(repo, "commit", "-q", "-m", "first");
    fs.writeFileSync(file, "= Title\n\nThe new sentence.\n");
    git(repo, "commit", "-q", "-am", "second");
    await vscode.window.showTextDocument(vscode.Uri.file(file));

    opened = [];
    errors = [];
    const window = vscode.window as any;
    const commands = vscode.commands as any;
    window.showQuickPick = async (items: vscode.QuickPickItem[]) => items[items.length - 1];
    window.showErrorMessage = async (message: string) => void errors.push(message);
    commands.executeCommand = (command: string, ...args: any[]) =>
      command === "vscode.open" ? void opened.push(args[0]) : original.executeCommand(command, ...args);
    await config.update("openInEditor", true, vscode.ConfigurationTarget.Global);
  });

  teardown(async () => {
    Object.assign(vscode.window, {
      showQuickPick: original.showQuickPick,
      showErrorMessage: original.showErrorMessage,
    });
    Object.assign(vscode.commands, { executeCommand: original.executeCommand });
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    // On Windows the test instance holds the folder open, so removing it throws EPERM
    // although the folder does disappear once the instance exits.
    try {
      fs.rmSync(repo, { recursive: true, force: true });
    } catch {}
  });

  test("produces and opens a PDF of the changes since the picked commit", async () => {
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await vscode.commands.executeCommand("typstRedline.showChanges");
    assert.deepStrictEqual(errors, []);
    assert.strictEqual(opened.length, 1);
    assert.strictEqual(fs.readFileSync(opened[0].fsPath).subarray(0, 5).toString(), "%PDF-");
  });

  test("the typst setting is passed to the CLI", async () => {
    await config.update("executable", binary, vscode.ConfigurationTarget.Global);
    await config.update("typst", "no-such-typst", vscode.ConfigurationTarget.Global);
    try {
      await vscode.commands.executeCommand("typstRedline.showChanges");
    } finally {
      await config.update("typst", undefined, vscode.ConfigurationTarget.Global);
    }
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
});
