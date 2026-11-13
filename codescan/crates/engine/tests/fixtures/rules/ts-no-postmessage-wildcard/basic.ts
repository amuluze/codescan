// ruleid: ts/no-postmessage-wildcard
window.postMessage(data, "*");

// ruleid: ts/no-postmessage-wildcard
frame.contentWindow.postMessage(payload, "*");

window.postMessage(data, targetOrigin);
worker.postMessage(data);
