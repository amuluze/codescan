// ruleid: js/no-postmessage-wildcard
window.postMessage(data, "*");

// ruleid: js/no-postmessage-wildcard
frame.contentWindow.postMessage(payload, "*");

window.postMessage(data, targetOrigin);
