import { defineConfig } from "@vscode/test-cli";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";

export default defineConfig({
  files: "out/test/**/*.test.js",
  // The tests make their repositories inside this folder, as when a folder of projects is open.
  workspaceFolder: fs.mkdtempSync(path.join(os.tmpdir(), "typst-redline-workspace-")),
  launchArgs: ["--disable-extensions"],
  mocha: { timeout: 60000 },
});
