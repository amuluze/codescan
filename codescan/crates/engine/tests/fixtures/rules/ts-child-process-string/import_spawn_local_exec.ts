import { spawn } from "child_process";

function exec(command: string) {
  return command;
}

exec("safe");
spawn("ls", []);
