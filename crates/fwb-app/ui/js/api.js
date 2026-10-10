// The only way the UI talks to the app: Tauri's invoke, through one queue so
// commands always run in the order the user made them.

const tauri = window.__TAURI__;

export const connected = Boolean(tauri && tauri.core && typeof tauri.core.invoke === 'function');

let queue = Promise.resolve();
let defaults = null;

/**
 * Arguments every command gets unless it names them itself: `fn()` returns
 * them when the command runs. Each window passes `doc`, the budget it is
 * showing (src/docs.rs), so a command sent just as another tab comes to the
 * front still lands on the budget the user was looking at. Commands that
 * don't take an argument ignore it.
 */
export function setDefaultArgs(fn) {
  defaults = typeof fn === 'function' ? fn : null;
}

/**
 * Runs a Rust command after every earlier one has finished. Resolves with its
 * result; rejects with a plain message string.
 */
export function invoke(cmd, args = {}) {
  const extra = defaults ? defaults() : null;
  const all = extra ? { ...extra, ...args } : args;
  const run = async () => {
    if (!connected) throw 'This window is not connected to Film Wizard Budgeting.';
    try {
      return await tauri.core.invoke(cmd, all);
    } catch (e) {
      throw typeof e === 'string' ? e : (e && e.message) || String(e);
    }
  };
  const p = queue.then(run, run);
  queue = p.catch(() => {});
  return p;
}
