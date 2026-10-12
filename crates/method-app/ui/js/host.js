// Which host the page runs in (host_info): the desktop app, or a browser
// running method-web (docs/research/web-session.md, phase 2). In a browser
// the page hides what only the desktop does: autosave, folder sharing, the
// updater, the start window and popping the tools out into a window.
//
// Until loadHost() has answered, and wherever host_info can't be asked, the
// host is the desktop: nothing changes there.

let info = { kind: 'desktop', read_only: false };

/** Asks the host once (through `invoke`, api.js's or a window's own). */
export async function loadHost(invoke) {
  try {
    const i = await invoke('host_info');
    if (i && typeof i === 'object' && typeof i.kind === 'string') info = i;
  } catch (e) {
    // An older desktop build: the desktop.
  }
  return info;
}

/** True in a browser (method-web). */
export function isWeb() {
  return info.kind === 'web';
}
