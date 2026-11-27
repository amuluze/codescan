import * as child_process from "child_process";

// ruleid: ts/child-process-string-member
child_process.exec("ls -la");
child_process.spawn("ls", []);

import { exec as runExec } from "child_process";

// ruleid: ts/child-process-string
runExec("ls -la");

const cp = require("child_process");
// ruleid: ts/child-process-string-member
cp.exec("ls -la");

const { exec: runAlias } = require("child_process");
// ruleid: ts/child-process-string
runAlias("ls -la");

function run(input: string): void {
  // ruleid: ts/child-process-string-member
  cp.exec(input);
}
