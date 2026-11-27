const target: Record<string, unknown> = {};
// ruleid: ts/prototype-pollution
target.__proto__ = payload;
// ruleid: ts/prototype-pollution
target["__proto__"] = payload;
target.safe = 1;
