// The only way the UI talks to the app: Tauri's invoke, through one queue so
// commands always run in the order the user made them.

const tauri = window.__TAURI__;

export const connected = Boolean(tauri && tauri.core && typeof tauri.core.invoke === 'function');

let queue = Promise.resolve();

/**
 * Runs a Rust command after every earlier one has finished. Resolves with its
 * result; rejects with a plain message string.
 */
export function invoke(cmd, args = {}) {
  const run = async () => {
    if (!connected) throw 'This window is not connected to Film Wizard Budgeting.';
    try {
      return await tauri.core.invoke(cmd, args);
    } catch (e) {
      throw typeof e === 'string' ? e : (e && e.message) || String(e);
    }
  };
  const p = queue.then(run, run);
  queue = p.catch(() => {});
  return p;
}
