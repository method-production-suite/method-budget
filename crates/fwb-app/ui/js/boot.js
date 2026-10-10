// Which page this is: the main window, or the tools window, which loads the
// same page with ?panel=1 and draws only the tools panel (src/tools.rs).

const panelOnly = new URLSearchParams(window.location.search).get('panel') === '1';
if (panelOnly) import('./toolswin.js');
else import('./main.js');
