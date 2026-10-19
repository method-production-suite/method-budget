// The website's demo (methodbudget.com/try/, docs/WEBSITE.md). Loaded
// before the shim, which reads the address: unless it names a budget
// already, the demo starts with the invented sample budget beside the page,
// Field Station, the one the desktop app opens as its sample
// (?open=sample/...). On the page itself (not in the Budget Setup frame)
// it shows the note about running in the browser, which can be hidden.

(function () {
  'use strict';

  const SAMPLE = 'sample/Field%20Station.mthd';
  const params = new URLSearchParams(window.location.search);
  if (window.parent !== window || params.has('panel')) return;

  if (!params.has('open')) {
    const search = window.location.search ? `${window.location.search}&` : '?';
    window.history.replaceState(null, '', `${search}open=${SAMPLE}${window.location.hash}`);
  }

  function show() {
    const note = document.getElementById('mw-demo');
    if (!note) return;
    note.hidden = false;
    document.body.classList.add('mw-demo-on');
    const close = document.getElementById('mw-demo-close');
    if (close) {
      close.addEventListener('click', () => {
        note.hidden = true;
        document.body.classList.remove('mw-demo-on');
      });
    }
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', show);
  else show();
})();
