# Chart sources

Where each bundled chart of accounts comes from (scripts/build-coa-templates.py).

## Our own charts

Written for Method Budget; the sources are committed in `templates/charts/src/`.

- `feature`: Feature film
- `low-budget-feature`: Low-budget feature
- `tv-one-hour`: TV episodic, one hour
- `tv-half-hour`: TV episodic, half hour
- `commercial`: Commercial
- `music-video`: Music video
- `documentary`: Documentary

## Studio and network charts

Studio and network charts of accounts: category and account numbers, names and production
totals.

- `abc-cable-network-group`: ABC CABLE NETWORK GROUP
- `ae-global-media-unscripted`: AE GLOBAL MEDIA UNSCRIPTED
- `american-cinema`: AMERICAN CINEMA
- `asmith-company`: ASMITH & COMPANY
- `academic-template`: Academic Template
- `academic`: Academic
- `bet-chart-of-accounts-template`: BET CHART OF ACCOUNTS TEMPLATE
- `brad-grey`: BRAD GREY
- `british-form`: BRITISH FORM
- `carsey-werner-television`: CARSEY-WERNER TELEVISION
- `cbs-television`: CBS TELEVISION
- `crusader-entertainment`: CRUSADER ENTERTAINMENT
- `disney-features`: DISNEY FEATURES
- `dreamworks-features`: DREAMWORKS FEATURES
- `dreamworks-television`: DREAMWORKS TELEVISION
- `dufferin-gate`: DUFFERIN GATE
- `eco-challenge`: ECO-CHALLENGE
- `evolution-film-tape`: EVOLUTION FILM & TAPE
- `goldheart`: GOLDHEART
- `hallmark-ent`: HALLMARK ENT
- `hbo-episodic`: HBO EPISODIC
- `hbo-master`: HBO MASTER
- `hearst-actuality`: HEARST ACTUALITY
- `hearst-ent-usa`: HEARST ENT USA
- `henson-features`: HENSON FEATURES
- `henson-prod-tv`: HENSON PROD TV
- `hit-entertainment`: HIT ENTERTAINMENT
- `king-world`: KING WORLD
- `mandalay-features`: MANDALAY FEATURES
- `mgm-animation`: MGM ANIMATION
- `mgm-features`: MGM FEATURES
- `mgm-television`: MGM TELEVISION
- `motion-international`: MOTION INTERNATIONAL
- `marvel`: Marvel
- `nbc-tv`: NBC TV
- `new-york-state-form-b`: New York State Form B
- `paramount-classics`: PARAMOUNT CLASSICS
- `paramount-domestic-tv`: PARAMOUNT DOMESTIC TV
- `paramount-features`: PARAMOUNT FEATURES
- `paramount-mow`: PARAMOUNT MOW
- `paramount-network-episodic`: PARAMOUNT NETWORK EPISODIC
- `paramount-viacom-episodic`: PARAMOUNT VIACOM EPISODIC
- `salter-st-corp`: SALTER ST CORP
- `sony-columbia-tristar-features`: SONY COLUMBIA TRISTAR FEATURES
- `sony-features-template`: SONY FEATURES TEMPLATE
- `telefilm-besi`: TELEFILM BESI
- `triage-entertainment`: TRIAGE ENTERTAINMENT

## Production charts

- `stg-pre-prelim-ky-tier-1`: STG Pre-Prelim KY Tier 1. Our own chart, from the STG pre-prelim budget (Kentucky, Tier 1), 2025-09-20.

## Public charts found, not bundled

Charts of accounts that public bodies publish for production incentive programs, found while
researching (2026-10-10). Each is a free public PDF with no terms of use stated on the page.
They were read for research only (our own charts were checked against them for coverage; no
numbering or wording was copied), not saved into `imports/coa-sources/` and not built:
downloading them needs the owner's go-ahead, and as PDFs they would first be converted to the
CSV layout in docs/TEMPLATES.md, keeping numbers, names and totals only (not their qualified
or not columns or comments).

| Chart | URL | Notes |
|---|---|---|
| California Film Commission, chart of accounts for films, movies of the week and miniseries (rev. March 2014) | https://cdn.film.ca.gov/wp-content/uploads/2020/11/1-QEC-Film.pdf | `101-00` categories, `101-01` accounts; above the line, production, post production, other. The importer reads `101-00` as category `101` |
| California Film Commission, independent projects and relocating TV series (program 2.0, rev. May 2017) | https://test.film.ca.gov/wp-content/uploads/2020/10/Yr-1-Independent-Relocating-TV-Chart-of-Accounts-2-0.pdf | Same numbering, TV sections |
| California Film Commission, one-hour TV episodic | https://archive.film.ca.gov/wp-content/uploads/2017/02/CA-Chart-of-Accts-1HrTVEpisodic.pdf | Archive copy; did not load |
| California Film Commission, relocating half-hour TV series | https://archive.film.ca.gov/wp-content/uploads/2017/02/CA-Chart-of-Accts-RelocatingHalfHourTVSeries.pdf | Archive copy; not read |
| California Film Commission, qualified expenditure charts index | https://film.ca.gov/tax-credit/qualified-expenditure-charts/ | The current program's charts |
| New York State, Empire State Film Production Credit report, appendix E (Form B) | https://www.tax.ny.gov/pdf/stats/policy_special/film_production_credit/report_on_the_empire_state_film_production_credit_august_2010_appendix_e.pdf | Line-by-line chart for film and television; not read in full |
| Tennessee Entertainment Commission, production incentive guidelines, appendix B | https://www.tnentertainment.com/lib/file/manager/PDFs/Download_the_Production_Incentives_Guidelines-8-11-16.pdf | `101-00` numbering; not read in full |
