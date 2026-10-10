// Which page this is: the main window; the tools window, which loads the
// same page with ?panel=1 and draws only the tools panel (src/tools.rs); or
// the Budget Setup window, with ?panel=setup, which draws only the setup
// tabs (src/setupwin.rs).

const panel = new URLSearchParams(window.location.search).get('panel');
if (panel === '1') import('./toolswin.js');
else if (panel === 'setup') import('./setupwin.js');
else import('./main.js');
