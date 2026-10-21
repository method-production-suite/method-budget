// Which page this is: the main window; the tools window, which loads the
// same page with ?panel=1 and draws only the tools panel (src/tools.rs); or
// the Budget Setup window, with ?panel=setup, which draws only the setup
// tabs (src/setupwin.rs). Each fills in index.html's words in the system's
// language first (i18n.js).

import { fillPage } from './i18n.js';

fillPage();

const panel = new URLSearchParams(window.location.search).get('panel');
if (panel === '1') import('./toolswin.js');
else if (panel === 'setup') import('./setupwin.js');
else import('./main.js');
