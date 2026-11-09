// ruleid: ts/no-timer-string
setTimeout("runJob('a')", 500);

// ruleid: ts/no-timer-string
setInterval(`refresh()`, 1000);

const id = setTimeout(() => refresh(), 1000);
clearTimeout(id);
