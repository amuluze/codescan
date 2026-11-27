import { spawn } from "child_process";

const db = { exec: (_command: string) => undefined };
db.exec("safe");
spawn("ls", []);
