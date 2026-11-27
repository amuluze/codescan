import * as child_process from "child_process";

const child_process = { exec: (_command: string) => undefined };
child_process.exec("echo shadowed module");
