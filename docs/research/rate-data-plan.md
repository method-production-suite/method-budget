# Rate data: what to ship, where it comes from, and how it reaches a budget

Status: research and plan, 2026-10-12. Nothing here is built or decided. Private notes for
the owner: they name third parties, discuss check-only material and quote figures that
change every year. Scrub or drop this file before the repository goes public (the
pre-launch scrub list). Every figure below was read from the cited page on 2026-10-12 and
is here to show what a source gives, not to be typed into a budget.

Feeds roadmap 3.9 (Rate tables), 3.19 (New project window) and later 3.6 (Incentive
modeling). Builds on decision 50 (setup packs), decision 54 (flat fringe basis), the
CLEAN-ROOM.md rules and the private data repository `method-budget-data`.

Contents:

0. Summary
1. Where we are today
2. What data to ship, and where each kind lands in a budget
3. Sources by data type
4. The ethical and legal line
5. Pipeline: from a public PDF to a reviewed pack
6. How the app consumes packs
7. Rate lookup and the New project window
8. Phases, effort and risks
9. Open questions for the owner
10. Sources

---

## 0. Summary

- **Ship five families of data, in this order of value per effort:** US payroll taxes
  (small, official, public domain, changes once a year), per diem and mileage (federal,
  public domain, an API and downloadable files), union fringe and benefit rates (already
  partly in the cards), union minimums by classification (the 97 cards, the biggest job
  because every shipped row needs a person to check it), and location incentives (last,
  with roadmap 3.6, because a wrong summary misleads a financing decision).
- **Facts are free, presentation is not.** Rates are facts (Feist, 1991) and federal data
  is public domain (17 U.S.C. 105). We take figures, classifications as printed, dates and
  page numbers, restate everything else in our own words, never copy a PDF, a layout or a
  third party's compilation, and attribute every figure to its public source.
- **Humans fetch, scripts extract, humans verify.** No crawler. A person downloads each
  document as an ordinary visitor; scripts extract; a second person checks every shipped
  row against the page. Two of the biggest union sites (SAG-AFTRA, IATSE) answered our
  automated fetch with HTTP 403 on 2026-10-12, which settles the question for them anyway.
- **Never a source:** paid payroll guides, login-gated member portals, production-held
  cards, insurance rating bureaus' licensed manuals (NCCI), and other companies' incentive
  maps or rate aggregators. The current "cross-checked against check-only material" status
  in the data repository is the one practice here worth rethinking (section 4.4).
- **A pack per agreement, region and contract year.** The data repository builds
  deterministic `.mthd` setup packs (decision 50) from verified cards, plus a small index
  the app reads to find the right packs. Packs ship inside app releases first: no new
  network code, and the signed updater already delivers them. A separate data channel is
  possible later and optional.
- **The format needs one new setup kind**, rate tables, for lookup. Fringes, globals and
  locations already carry the rest, with `setup_sources` saying where each came from.
- **First useful slice** (phases 0 to 2, roughly two to three weeks of agent work plus the
  owner's or a verifier's checking time): payroll tax packs for the six routed US hubs'
  states, a per diem and mileage pack, and verified crew packs for Los Angeles, New York
  and Atlanta. Lookup (3.9) and the New project window (3.19) follow.

## 1. Where we are today

**In the app.**

- A setup pack is a `.mthd` with setup and no lines, provenance `setup_pack` with a title,
  description, author, version, effective date, region, notes and links (decision 50,
  docs/FORMAT.md "Setup packs"). Importing one runs Import Setup's review: new items are
  added, items set up differently keep the budget's version unless the user takes the
  pack's, one undo step, and every item is stamped in `setup_sources` with the pack's
  title, version and effective date plus per-item notes and links.
- Packs are listed from the user's Packs folder and the bundled `packs/` folder, which
  ships empty but for a README saying rate packs come from a separate data project after
  review.
- What a pack can carry that matters here: **percent fringes** with a per-earner `cutoff`
  in cents (payroll taxes, vacation and holiday percentages); **flat fringes** per unit
  with `basis` and `max_hours_per_day` (hourly pension and health with a daily cap, daily
  aggregate contributions; decision 54); **globals** with a description, unit and folder
  (per diem, mileage, scale date); **locations** (description, city, region, country);
  overtime patterns (every number the user's). There is no rate table kind yet.
- A line's `rate` is a formula, so it can hold a number or a global's name.
- The only network access is the signed updater (decision 48), on by default with a menu
  item to turn automatic checks off. The web view has no network at all.

**In the data repository** (`method-budget-data`, private).

- 97 cards (one JSON per public source document) in four regions, 3,220 tables, 61,885
  rows, 306 fringe entries; schema 1.0 with tables, `applies_to` (media, budget tier,
  show shape, hubs, area), effective dates, rows with page numbers, fringes with a basis,
  and flags (`open`, `estimate`, `not_modelled`, `judgment`).
- Review status: 81 `unreviewed`, 16 `cross_checked`, 0 `verified` (docs/COVERAGE.md).
- US jurisdiction routing as data: zip to hub, budget tiers per union, TV show shapes,
  default agreements, hub crafts, Local 161, ICG 600 regions, Teamsters, date rules
  (docs/JURISDICTION.md). Six US hubs are routed.
- Sourcing rules already match most of section 4: public sources only, a SHA-256 and
  retrieval date per document, no agreement prose, source files gitignored, check-only
  material never committed.
- Some sources are re-hosted copies on third-party sites (a Swiss film body hosting UK
  PACT and Equity rates, a UK extras agency, a blog host, a cinema guild site). Of the 97
  cards, 20 come from amptp.org and most of the rest from the issuing union or producer
  association.

## 2. What data to ship, and where each kind lands in a budget

| Data | Lands in | Notes |
|---|---|---|
| Union minimums by classification | **New: rate tables** (section 6.2); a fill writes a line's rate | The cards' tables as they are: classification as printed, code, department, rate, unit, guaranteed hours, page, effective dates, applies-to. |
| Vacation and holiday, other percent benefits | Percent fringes | Some agreements print one combined percent; some split by daily and weekly hires. |
| Pension and health, annuity, training trusts per hour or per day | Flat fringes (`unit` Hour or Day, `basis`, `max_hours_per_day`) | Exactly what decision 54 built. The cards' `per_hour_worked`, `per_day`, `per_week` bases map onto it; `percent_of_scale` and `percent_of_gross` map onto percent fringes (scale versus gross is a known approximation, flag it). |
| Social Security, Medicare (employer side) | Percent fringes, Social Security with a cutoff | 2026: 6.2% up to $184,500; Medicare 1.45% with no cap. The extra 0.9% Medicare is employee-only, so it is not a production cost. |
| FUTA | Percent fringe, cutoff $7,000 | 6.0% less the 5.4% state credit is 0.6%; states with a credit reduction pay more (2025: California 1.2%, US Virgin Islands 4.5%, final in November and retroactive). |
| State unemployment (SUTA) and state employer taxes | Percent fringes with the state's wage base as cutoff | Rates are experience-rated per employer, so ship the new-employer rate as a default and say so. Example, California 2026: UI new-employer 3.4% and ETT 0.1%, each on the first $7,000. |
| State disability and paid leave | Only the employer share, as percent fringes | California SDI (1.3% in 2026, no wage limit) is withheld from the employee, so it is not a budget cost. Several states have employer shares; check each. |
| Workers' compensation | A percent fringe the user fills in | Rates come from the production's insurer or payroll company. Ship the item with no figure (section 3.4). |
| Per diem and lodging | Globals, or a rate table keyed by location | GSA's federal rates (FY 2027 standard: $113 lodging, $68 meals and incidentals, 295 non-standard areas) and the agreements' own per diem clauses. |
| Mileage | A global with unit Mile | IRS 2026 business rate: 72.5 cents. Many agreements tie mileage to the IRS rate. |
| Minimum wage (non-union floors) | Globals per state or city | DOL's table, updated 1 January and 1 July. |
| Non-union reference wages | A rate table marked "reference, not a minimum" | BLS OEWS by occupation and metro. Weak fit (few film-specific occupations); low priority. |
| Location incentives | Later: roadmap 3.6's program model. Until then, a location's notes and links at most | Program, base percent, uplifts, caps, minimum spend, application windows, effective dates, official link. |
| Exchange rates (adjacent) | Currencies | Out of scope here; the Federal Reserve's H.10 series and ECB reference rates would be the official sources if wanted. |

Known approximation: a payroll tax wage base resets per employer per calendar year, while
a budget's percent cutoff is per earner for the whole budget (decision 3). A shoot that
crosses 1 January, or one person paid in several accounts (roadmap 3.11), will be off.
Packs should say so in their notes.

## 3. Sources by data type

Each row: who publishes it, what it permits as far as we could see on 2026-10-12, and how
often it changes. "robots" means the site's robots.txt. A 403 is what the server answered
to an automated fetch.

### 3.1 Union minimums and benefits

| Source | What it gives | Terms and robots (2026-10-12) | Cadence | Verdict |
|---|---|---|---|---|
| AMPTP (amptp.org), the producers' association | Full agreements and wage scale PDFs for IATSE locals, Basic Crafts, Teamsters, AFM, WGA wage tables, others | No terms-of-use page found; footer "All Rights Reserved"; robots.txt allows everything (Yoast default) | Per agreement; IATSE steps each August (2024 Basic Agreement: 7% from 4 Aug 2024, 4% from 3 Aug 2025, 3.5% from 2 Aug 2026) | Best US source: it is one of the two parties to the agreements. Human download, extract facts. |
| Union and local sites (IATSE locals, ICG 600, Teamsters 399, Local 52, 161, 798, 829 and others) | Rate cards, wage schedules, fringe sheets | Vary by site. iatse.net answered 403 | Per agreement step; some fringe rates change mid-year | Use the issuer's public copy; never a member-only page. |
| SAG-AFTRA (sagaftra.org, sagindie.org) | Theatrical, TV and low-budget rate sheets | Online policies dated 2017 cover user submissions; the site answered 403 to automated requests, including robots.txt | New contract in 2026; annual steps | Human download only. |
| DGA (dga.org) | Basic Agreement wage tables, rate cards | Footer "Terms and Conditions"; robots.txt disallows only `/account/` and a members' news path | Contract 2026 to 2030 per the card | Human download, extract facts. |
| WGA (wga.org) | Schedule of Minimums, MBA wage tables | robots.txt disallows two negotiation PDFs only | Contract 2026 to 2030 | Human download, extract facts. |
| Benefit plans (Motion Picture Industry Pension and Health Plans, DGA, SAG-AFTRA and WGA plans) | Employer contribution rates | Plan sites have their own terms; the SAG-AFTRA plans' terms allow a single personal copy and bar commercial use | Can change mid-year: a payroll firm reported the East Coast MPI hourly rate rising on 22 March 2026 | Take figures from the agreement or the plan's public notice; never copy plan documents. |
| Canada, UK, Ireland, Europe, Australia, New Zealand unions and producer bodies | Rate charts and agreements (ACTRA, AQPM, IATSE Canadian locals, PACT, Equity, MU, WGGB, ver.di, Spanish BOE, French collective agreements and others) | Vary; government gazettes (BOE) carry official texts | Per agreement | Same rules; prefer the issuer's copy over re-hosts. |
| Payroll company rate guides, online labor guides, rate aggregators | Compiled rates across unions | Paid, login-gated or the compiler's copyrighted compilation | Annual | **Never a source.** See 4.4 for checking. |

### 3.2 Payroll taxes

| Source | What it gives | Terms | Cadence | Verdict |
|---|---|---|---|---|
| SSA, "Contribution and benefit base" | Social Security wage base ($184,500 for 2026) and rates | Federal work, public domain | Announced in October with the COLA, effective 1 January | Ship. |
| IRS (Publication 15, Form 940 and Schedule A, Additional Medicare Tax FAQ) | Rates, FUTA credit, Medicare | Public domain | Annual; Schedule A list each January | Ship. |
| DOL Employment and Training Administration, "Significant Provisions of State UI Laws" (January 2026 edition) | Every state's taxable wage base, rate range and new-employer rate in one table | Federal work, public domain | Periodic editions (this one January 2026) | Ship as the starting point; confirm each state against its own agency. |
| DOL, FUTA credit reduction notice (Federal Register) | Which states pay a higher FUTA rate | Public domain | States at risk known early in the year, final in November, retroactive to 1 January | Ship as a note and, once final, the adjusted rate. |
| State labor and tax agencies (EDD California, NY DOL, Georgia DOL and others) | SUTA, training taxes, disability and paid leave shares | State works: copyright depends on state law (17 U.S.C. 105 covers only federal works); the figures are still facts | Annual, some states mid-year | Ship the figures with a link; never copy the page text. |
| HMRC, "Rates and thresholds for employers 2026 to 2027" | Employer Class 1 National Insurance 15% above £5,000 a year | Open Government Licence v3.0 (page footer; last updated 1 Sept 2026) | Tax year from 6 April | Ship for UK packs, with the OGL attribution line. |
| CRA (Canada) | CPP (5.95%, maximum pensionable earnings $74,600, $3,500 exemption), CPP2 (4% to $85,000), EI (1.63% employee, employer 1.4 times, maximum insurable $68,900) for 2026 | Crown copyright on the pages (not checked in detail; it governs text, not the figures) | 1 January, announced in November | Confirm on CRA's own tables (our figures came from secondary sources). |

### 3.3 Per diem, mileage, minimum wage, reference wages

| Source | What it gives | Terms | Cadence | Verdict |
|---|---|---|---|---|
| GSA per diem API (open.gsa.gov) and gsa.gov/perdiem files | Lodging and meals by city, state, ZIP and fiscal year; meals broken into breakfast, lunch, dinner, incidentals | Free key, 1,000 requests an hour; GSA's developer terms ask users to cite the retrieval date, say GSA cannot vouch for derived analyses, not present modified data as GSA's, and not use GSA logos. No controls on end use | Federal fiscal year from 1 October; new rates mid-August (FY 2027 notice published 10 Sept 2026) | Ship. The data repository fetches once a year with its own key; the app never calls the API (a key in a shipped app would be shared with every user). |
| State Department Office of Allowances | Foreign per diem by city, lodging and meals | Public domain; Excel files | Monthly | Ship for international packs, dated by month. |
| IRS standard mileage rates (IR-2025-128, Notice 2026-10) | 72.5 cents a mile for 2026 business use | Public domain | Announced late December | Ship. |
| DOL Wage and Hour, state minimum wage table | State minimums (California $16.90 in the July 2026 table) | Public domain | Updated 1 January and 1 July; cities set their own | Ship state figures; city ordinances only with the city's own page. |
| BLS OEWS (Public Data API or bulk tables) | Wage percentiles by occupation and metro | Public domain; API terms ask for the retrieval date, say BLS cannot vouch for derived data, and put no controls on end use; keyed API 500 queries a day | Annual (May reference period) | Optional, low priority, always labeled "reference". |

### 3.4 Workers' compensation

| Source | What it gives | Terms | Verdict |
|---|---|---|---|
| NCCI loss costs and classification manuals (most states) | Loss costs by class code | Copyrighted and licensed; filings say no copying, derivative works or posting without NCCI's written permission | **Never.** |
| WCIRB (California) and other independent bureaus | Advisory pure premium rates by class (California's 1 Sept 2026 benchmark: $1.65 per $100 of payroll, per the Insurance Commissioner's decision of 10 July 2026) | Not checked yet | Not before reading the bureau's terms, and even then only as a labeled reference: a production's actual rate comes from its carrier or payroll company. |
| Payroll companies' quoted rates | All-in rates per class | Proprietary | Never; the user types their own. |

### 3.5 Location incentives

| Source | What it gives | Terms | Cadence | Verdict |
|---|---|---|---|---|
| State film offices (Georgia Film Office on georgia.org: 20% transferable credit plus a 10% promotional uplift, no cap; California Film Commission, Program 4.0 guidelines on film.ca.gov) | Program rules, rates, caps, uplifts, application windows | State works; facts free, page text not ours to copy | Legislative sessions; application windows several times a year | Ship structured facts with links, in our words, with roadmap 3.6. |
| National programs (Canada's federal and provincial credits, UK AVEC and the independent film credit, Ireland Section 481, Australia's offsets and others) | Same | Government licences vary (UK OGL is permissive) | Budget cycles | Same. |
| Payroll companies' and vendors' incentive maps and guides | Compiled summaries | Their copyrighted compilations | Frequent | **Never a source.** At most a reminder of which jurisdictions have a program; every fact is then taken from the official page. |

## 4. The ethical and legal line

The owner has declined a lawyer, so this section states the risks plainly instead of
recommending one. None of it is legal advice.

### 4.1 What the law gives us

- **Facts are not copyrightable.** A rate is a fact. A compilation of facts is protected
  only in its original selection and arrangement, and that protection is "thin": a later
  compiler may use the facts as long as it does not copy the selection and arrangement
  (Feist v. Rural Telephone, 499 U.S. 340, 1991). Our cards restructure every table into
  our own schema, so we take facts, not arrangement.
- **Short phrases** such as "Key Grip" or "Dolly Grip" are not protected; we keep
  classifications exactly as printed because that is what payroll matches on.
- **Federal works are public domain** (17 U.S.C. 105): SSA, IRS, DOL, GSA, BLS and State
  Department data. State works depend on state law; the facts in them are still free.
  Official legal texts are public under the government edicts doctrine (Georgia v.
  Public.Resource.Org, 2020), which on our reading does not reach privately negotiated
  union agreements.
- **Robots.txt is a request, not access control** (RFC 9309 says the rules "are not a form
  of access authorization"). We honor it anyway, as a courtesy and because ignoring it is
  evidence of bad faith.
- **Terms of service** bind most clearly where someone agreed to them, such as signing in:
  the court held that hiQ breached LinkedIn's user agreement by scraping and by using fake
  accounts (November 2022; the case then ended in a consent judgment), while another court
  read Meta's terms as not covering logged-off collection of public data (Meta v. Bright
  Data, N.D. Cal., 2024).
  That is a reason to never sign in to get data, not a licence to scrape public pages.
- **Europe has a database right** for makers who invested substantially in obtaining,
  verifying or presenting contents (Directive 96/9/EC, article 7). The Court of Justice
  held that investment in *creating* the data does not count (British Horseracing Board v.
  William Hill, C-203/02, 2004), which helps us with a union's or producer body's own rate
  tables and hurts us with any third party's compiled database. We take neither a
  "substantial part" of a third-party database nor anything from one at all.
- **Spain**: article 13 of the Intellectual Property Law excludes legal and regulatory
  provisions and acts of public bodies from protection. Whether a collective agreement
  published in the BOE falls under that is not settled by our reading; its figures are
  facts either way.
- **UK government data** is mostly under the Open Government Licence v3.0, which allows
  commercial reuse with an attribution statement.

### 4.2 Do

- Prefer the **issuer's own copy**: the union, the guild, the producers' association, the
  agency. When only a re-host exists, record it as a secondary copy and look for the
  original each refresh.
- Prefer **official APIs and downloadable files** over pages (GSA's API and files, BLS
  bulk tables, State Department Excel files, DOL's PDF table).
- **Download as a person**, in a browser, as an ordinary visitor. Record the URL, the date
  and the file's SHA-256 (the data repository already does).
- **Take facts only**: figures, classifications as printed, codes, units, dates, page
  numbers, which tiers and shapes a table covers. Restate scope rules and notes in our own
  words.
- **Attribute every figure**: each pack and item names its source title, link, page and
  retrieval date. For GSA and BLS add their requested line ("retrieved on date X; GSA/BLS
  cannot vouch for data derived from it"). For OGL data add the OGL attribution.
- **Say what it is not**: minimums as printed, not legal or payroll advice, not affiliated
  with or endorsed by any union, guild or agency; check figures against the source and the
  production's payroll company before relying on them.
- **Use names, never logos**: "IATSE Local 600 rates, Western Region" is a factual
  description. No union marks, no colors, no wording that suggests endorsement.
- **Cache politely**: one copy per document per refresh, kept privately; any automated
  check of a known URL at most monthly, one request at a time per host, an honest
  User-Agent with a contact address, honoring robots.txt and backing off on any 403 or 429.
- **A person verifies** every shipped row against the page, and a second person (or the
  extracting script plus one person) signs off before a pack ships.
- **Fix fast**: a public way to report a wrong figure with the page reference, and a
  patched pack in the next release.

### 4.3 Don't

- Don't crawl union or agency sites, and don't use headless browsers, rotating proxies or
  "stealth" services to get past a 403. A block is an answer.
- Don't sign in to anything to get data: member portals, payroll platforms, plan
  participant sites.
- Don't use paid guides as a source, don't copy from them, and don't scrape them.
- Don't copy or redistribute the PDFs, page images, layouts or agreement prose. The packs
  carry figures; the link points to the source.
- Don't take anything from another company's compilation: payroll companies' rate books,
  incentive maps, labor guides, aggregator sites. Not even "just the numbers", because
  their selection is the protected part and their numbers may be their own errors.
- Don't ship NCCI material, ever, or any bureau's rates before reading its terms.
- Don't ship anything a person has not verified (exception for private testing builds).
- Don't put anyone's private name in a card, pack or commit: roles or handles only
  (already the data repository's rule), and no names tied to the owner's businesses in
  any public pack (the public anonymity rule).

### 4.4 Risks, stated plainly

| # | Risk | Likelihood | Harm | What we do |
|---|---|---|---|---|
| 1 | A union or guild objects to its rates appearing in a commercial-looking app | Low | A takedown request, bad press in a small industry | Facts plus attribution plus no logos is defensible; comply quickly with any request anyway; consider asking the big unions up front (question 13). |
| 2 | A wrong figure prices a budget short, or a production underpays someone | Medium | The real risk: credibility, possibly a claim | Verified-only shipping, flags shown at lookup, review status visible, disclaimers in every pack and the lookup, fast corrections. Software licences already disclaim warranty; say the same for data. |
| 3 | **Check-only material.** The data repository records `cross_checked` when figures were compared against a purchased payroll guide or production-held cards | Medium | A purchased guide's licence may limit use to the buyer's own reference, and using it to check a product's data could breach it; production-held cards may be confidential to that production. Recording the check (and its review status) leaves evidence | Ship only `verified` (checked against the public source), never `cross_checked` alone. The owner reads the guide's purchase terms; if they restrict this use, stop cross-checking against it and drop the status from shipped data. Public duplicates (section 5.4) give a legitimate cross-check instead. |
| 4 | Terms of a source site forbid commercial reuse of "content" | Low | Breach of contract claim, mostly theoretical without an agreement | Take facts only, download as a visitor, never sign in, keep a hosts registry of each site's terms and robots (section 5.1). |
| 5 | Europe's database right over a third-party compilation | Low | Injunction in the EU | Use only the rate-setting body's own documents. Several European cards come from re-hosts; replace them. |
| 6 | Trademark or false endorsement from union names | Low | Demand letter | Nominative use only, no logos, explicit "not affiliated". |
| 7 | Incentive summaries mislead a financing decision | Medium once shipped | Real money | Wait for 3.6; official links on every program; effective dates; "verify with the film office" on every result. |
| 8 | Workers' comp figures from licensed manuals | Low (we won't) | Copyright claim | Placeholder fringe only. |
| 9 | Stale data in old app installs | High over time | Wrong rates on a new budget | Effective dates on every table, staleness warnings against the scale date (section 6.4), terms that freeze rather than roll forward. |
| 10 | The anonymity rule: author names in packs, commit identities, docs | Medium | The owner's stated worry | Pack author "Method Budget rate data", the neutral commit identity the data repository already uses, scrub docs that ship. |

## 5. Pipeline: from a public PDF to a reviewed pack

```
issuer site --(person, browser)--> shared library folder (private, gitignored)
   --> extract (pdftotext -layout, per-document configs; OCR only for image-only PDFs)
   --> card JSON (unreviewed) --> validate (schema, dates, ranges, duplicates, new checks)
   --> verify (a person, page by page; row-level) --> diff review on every update
   --> pack build (deterministic) --> app repository packs/ --> app release
```

### 5.1 Acquire

- A person downloads from the issuer, saves into the matching region folder, and records
  the link (the existing `INDEX.md` and `source-index.json` flow).
- **New: a hosts registry** (`data/hosts.json`): for each host, the terms URL if any,
  robots.txt status, whether automated requests are welcome (none, HEAD only, fetch), the
  date checked and by whom (handle). Seeded from section 3: amptp.org (no terms page,
  robots allows all), dga.org (terms in footer, robots disallows `/account/`), wga.org
  (robots disallows two PDFs), sagaftra.org and iatse.net (403 to automation: none).
- **Optional: a change watcher**, only for hosts marked as allowing it: a monthly
  conditional GET (ETag or Last-Modified) on each known URL to say "this document
  changed, someone should look". Never discovers new URLs, never follows links. Needs the
  owner's yes (question 10).
- Federal data (GSA, IRS, SSA, DOL, BLS, State Department) is fetched by script from the
  official files or API, with the retrieval date stored. That is what those services are
  for.

### 5.2 Extract

- The existing tooling stays: Python standard library, `pdftotext -layout`, one config per
  document, cards written with `extraction.method` `script` or `manual`.
- Image-only PDFs (the ICG Central Region panels) may be OCR'd from the public file; an
  OCR'd card is always `manual` and always needs full verification.
- Payroll taxes, per diem and mileage get their own small card shapes (schema 1.1,
  additive): a `tax` table (jurisdiction, tax, payer employer or employee, rate, wage
  base, effective year, source page) and a `rates_by_location` table (location key,
  lodging, meals, incidentals, fiscal year). They are small enough to type by hand and
  verify twice.

### 5.3 Validate (automatic)

Already: schema, dates, rate ranges, duplicates, source links, the em dash check. Add:

- **Step-increase check.** Most agreements print their general increases. Each row in a
  later period should equal the earlier row times the increase, within the agreement's
  rounding (the 2024 IATSE Basic Agreement: 7%, then 4%, then 3.5%). Rows that don't are
  listed: either an extraction error or a real special increase worth a note.
- **Tier order.** Within one card, a higher budget tier is never lower than a lower tier
  for the same classification and unit.
- **Unit sanity.** Where a card prints hourly and weekly for the same job, weekly divided
  by the guaranteed hours lands near the hourly rate.
- **Diff report.** Re-extracting writes old and new figures side by side with the percent
  change, so a reviewer reads only what moved.

### 5.4 Verify (people)

- **Row-level review** (schema 1.1, additive): each row gets `verified_on` and
  `verified_by` (a handle). A card's status becomes derived: `verified` when every shipped
  row is. Re-extraction keeps the mark on rows whose figures did not change, so an update
  costs only the changed rows, not the whole card.
- **A local review page** in the data repository: the PDF page image on one side, the
  extracted rows for that page on the other, a key to mark each row right or wrong, and a
  note box. It writes only the review fields. This is the biggest lever on cost: 61,885
  rows cannot be checked any other way.
- **Two people.** The extractor (a script or an agent) never verifies its own output. Rows
  typed by hand need a second person.
- **Public duplicates as a free cross-check.** Several documents exist twice in public
  (the ACTRA chart from actra.ca and aqpm.ca, the AQTIS agreement in English and French,
  IATSE 212's two documents, Local 849's agreement and its Schedule A, Local 873's two
  files). Where two public copies agree row for row, that is real evidence and needs no
  check-only material.
- **Verify only what ships.** The first packs need a few thousand rows, not 62,000.

### 5.5 Version and date

- Cards keep one file per document; a new contract year is a new card or a new table with
  its own `effective_from` and `effective_to`.
- **New: a series key** on each card (schema 1.1), stable across years
  (`icg-600-western-region`), so a newer card can be recognized as the successor of an
  older one. Packs carry it too.
- Pack versions: `<first effective year>.<revision>` (`2026.1`, then `2026.2` for a
  correction). Pack titles name the agreement, the region and the period.
- A table's dates follow the date rule already written in docs/JURISDICTION.md: the scale
  date is the first day of principal photography (else today); the latest dated row on or
  before it wins; after a term ends the rates freeze and are reported stale, never rolled
  forward; a priced budget keeps what it was priced with.

### 5.6 Build packs

- **One pack per agreement, region and contract year.** Examples: "IATSE Local 600,
  Western Region, 2026-27"; "SAG-AFTRA Theatrical, 2026"; "US payroll taxes, California,
  2026"; "Per diem and mileage, US, FY 2027".
- A union pack holds its rate tables (the new kind), its fringes (percent and flat, with
  basis and daily caps), and any globals the agreement prints (per diem, meal money).
  Every item gets a `setup_sources` entry with the page and link. The provenance `pack`
  fields: title, description (scope in our words, flags, the disclaimer), author "Method
  Budget rate data", version, effective, region, notes, urls.
- A payroll pack per state per year: Social Security, Medicare, FUTA (with any credit
  reduction note), the state's unemployment and training taxes at the new-employer rate
  with its wage base as cutoff, employer disability or leave shares, and an empty
  workers' comp fringe. A federal-only pack for anywhere else.
- **Deterministic**: same cards, same bytes, as `scripts/build-coa-templates.py` does for
  chart templates. A test rebuilds every pack and compares.
- **Refuses** any row not verified, any card whose host is marked "none" for reuse, and
  anything carrying a private name.
- Also writes **`packs/rates-index.json`**: one entry per pack (series, organization,
  local, agreement, hubs, country and region, media, budget tiers, show shapes, effective
  dates, version, review summary, flags count) plus the routing tables the New project
  window needs (hubs, zip to hub, tiers, show shapes, default agreements), restated in our
  words. The app reads this index instead of opening every pack.
- Output goes to the app repository's `packs/` by pull request, listed in a
  `packs/SOURCES.md` like `templates/charts/SOURCES.md`.

## 6. How the app consumes packs

### 6.1 Delivery: three options

| Option | How | New network? | Freshness | Effort |
|---|---|---|---|---|
| A. Bundled in app releases | Packs in `packs/`, shipped with each release; a data refresh is a patch release through the existing signed updater | None | As often as we release | S |
| B. Separate data channel | A signed `rates-latest.json` manifest and pack files as assets on the same GitHub releases host the updater already uses, with a separate signing key; packs land in an app-managed "Downloaded packs" folder | A second request path in Rust; same host, same https-only rules | Any time | M |
| C. Manual | Packs as downloads on the website; the user drops them into the Packs folder (Show Packs Folder exists) | None in the app | When the user bothers | S |

**Recommendation: A now, C as a fallback, B only if release cadence ever lags the data.**
A keeps decision 48's single network path and its signature checks, needs no new code, and
data changes are infrequent and seasonal (section 8.3). If B is built: its checks ride the
same "Check for Updates Automatically" setting (or default off, owner's call, question 3),
the manifest is refused unless signed by the data key, packs are only ever added (never
silently replacing what a budget uses), and SECURITY.md and docs/APP.md say so.

Bundled and user packs with the same series: show both, newest version first, "comes with
the app" or "yours" as today.

### 6.2 The format: rate tables as a setup kind

The lookup needs the tables somewhere. Two shapes:

- **In the budget file** (`rate_tables`, a new setup kind, tools not content like tag
  presets: versions never carry them, restoring keeps them). A pack carries them, Import
  Setup copies them, the library holds them, `setup_sources` stamps them. A table is the
  card's table: name, series, organization, local, agreement, applies-to, effective dates,
  currency, rows (classification, code, department, rate, unit, guaranteed hours,
  negotiable, notes, page), flags and review summary. A budget would hold only the tables
  its production needs (tens to hundreds of rows), not the whole library.
- **Outside budgets**: tables live only in packs and the library; the lookup reads them
  there; budgets keep only filled figures and line stamps. Smaller budgets, but a budget
  sent to someone without the pack can't show its lookup.

The first is simpler to build on what decision 50 made and keeps a budget self-contained.
Either way it is an additive format change (question 5).

### 6.3 What a fill writes

- **Option 1, a number.** The line's `rate` gets the figure and `unit3` the unit; a small
  new optional field on the detail row (`rate_source`: pack series, table, classification,
  effective date) records where it came from. Simple, matches how budgets look today.
- **Option 2, a global.** The fill creates (or reuses) a global named for the
  classification (`L600_1ST_AC_WK`) with its value, unit, folder and source, and the line's
  rate becomes that name. A newer pack then updates every line through Import Setup's
  existing "different here" review. Many globals in big budgets; the grid shows names
  unless "show by name" is off.

Either is a design call for the owner and the orchestrator (question 6). Option 1 plus a
"review rate changes" panel (6.4) is closer to how line producers work.

### 6.4 Staleness

- A budget needs a **scale date**: a budget info field or a global ("first day of
  principal photography"), defaulting to the day the budget was made.
- A line priced from a table whose `effective_to` is before the scale date, or whose
  series has a newer table installed, is **stale**. The app says so quietly (a mark, a
  count in the Rates tab), never changes a number on its own.
- **Review rate changes**: lines and setup items from older tables, old figure, new
  figure, percent change, tick and apply as one undo step. For fringes and globals this is
  Import Setup's review on the newer pack, which already exists.
- A table that is past its term with no successor is shown as "last published rates,
  ended on date X". Never projected forward unless the user types an escalation.

## 7. Rate lookup and the New project window

Ideas for the owner and the orchestrator to choose from. No design decision is made here.

### 7.1 Rate lookup (roadmap 3.9)

- **Where**: a Rates tab in the tools panel next to the Library tab; a lookup from the Rate
  cell (right-click "Look up rate…" or a shortcut); the command line `method rates find`
  and an MCP tool for agents.
- **Narrowing**: union or guild, then local, then table (feature or TV, tier, show shape,
  period), then classification; or a search box across every installed table ("key grip")
  with results sorted by the budget's hub and scale date.
- **Each result shows**: rate and unit, guaranteed hours, the table and period, page,
  review status, flags (open, estimate, not modelled, judgment) as warnings, and the
  source title and link as text (shown, never opened, as decision 50 does for pack links).
- **Fill**: rate only, or the line (classification as description, unit, rate, the
  agreement's fringes ticked on). Multi-line fill for a selected crew block.
- **Unverified data** never appears in a release build; a private testing build can show
  it greyed.
- **Non-union lines**: the minimum wage global for the location and, if shipped, a BLS
  reference row clearly labeled.

### 7.2 New project window (roadmap 3.19)

The window already plans title and details, chart or template, schedule length, currency,
and "union or location rate setup". The rate part, in steps:

1. **Where**: a hub, or city, state and country (a ZIP for US routing).
2. **What**: feature or TV; for TV the show shape (pilot or long-form, new one-hour,
   one-hour season 3 on, new half-hour, other).
3. **How big**: the budget figure each union tiers on (the routing data knows that LBTA
   and DGA take out bond and contingency; SAG-AFTRA, WGA and Teamsters read the full
   figure). One number in, each union's tier shown.
4. **Who is union**: per union or guild, union or not; "all union", "all non-union" and
   "mixed" as shortcuts.
5. **When**: first day of principal photography (the scale date) and shoot length (the
   schedule globals).

From these the window reads `rates-index.json`, routes through the jurisdiction tables, and
proposes a list of packs: the crew agreement and locals for that hub, tier and format; the
guild packs; the state payroll pack; per diem and mileage; an empty workers' comp fringe.
Each line shows its flags (for example "no Teamster driver sheet on file for Atlanta",
"Illinois collar counties routed to Local 476, kept for a recheck") and can be unticked.
The budget then opens with those fringes, globals, locations and tables in place, stamped
with their sources, and with lines empty (rates fill as lines are written).

A one-time note in the window: minimums as printed, not payroll or legal advice, check with
the production's payroll company. Location incentives show as a link and a one-line
summary per program until 3.6 exists.

## 8. Phases, effort and risks

S is about a day of agent work, M several days, L a week or more. Verification time is a
person's time and is listed separately.

### 8.1 Phases

| Phase | What | Where | Effort | Person time | Risks |
|---|---|---|---|---|---|
| 0. Hygiene | Hosts registry; replace re-hosted sources with issuers' copies where they exist; decide the check-only policy; scrub names from anything that will ship; add the disclaimer text | data repo | S | an hour of owner decisions | Some documents have no issuer copy. |
| 1. Verification tooling | Row-level review fields (schema 1.1), the local review page, step-increase and tier checks, diff report, public-duplicate comparison | data repo | M | none | The review page is only worth it if people actually use it. |
| 2a. Payroll, per diem and mileage data | Tax and location card shapes; federal 2026 figures; six states (CA, NY, GA, NM, LA, IL) for 2026; GSA FY 2027 and IRS 2026 | data repo | S to M | about a day, twice-verified | Mid-year state changes; experience-rated SUTA means defaults only. |
| 2b. Format and pack builder | `rate_tables` setup kind (format, reader, writer, Import Setup, library, packs, CLI `method pack show`), deterministic builder, `rates-index.json`, `packs/SOURCES.md`, the verified-only gate | app repo and data repo | M to L | none | Format choice (6.2) and fill choice (6.3) need deciding first. |
| 2c. First verified crew packs | LA, NY, Atlanta crew locals and agreements for 2026-27, plus SAG-AFTRA, DGA, WGA 2026 | data repo | M | several days of page-by-page checking | Volume; the IATSE agreements expire in mid-2027, so this set has a known shelf life. |
| 3. Rate lookup (3.9) | Rates tab, lookup from the Rate cell, fill, line source stamp, flags, staleness marks and the review panel, CLI and MCP | app repo | L | live check | UI choices are the owner's; fill semantics must not surprise. |
| 4. New project window (3.19) | Location, format, size, union status, dates; routing from the index; pack proposals; open ready to fill | app repo | M | live check | Routing is only as good as the jurisdiction tables, which carry open questions. |
| 5. Data channel (optional) | Option B in 6.1 | app repo | M | none | A second network path and key to guard. |
| 6. International | UK (OGL), Canada (CRA), Ireland, Australia payroll; State Department foreign per diem; the verified Canadian and UK union packs | both | M each | days per country | Crown copyright terms; many more documents in French and German. |
| 7. Incentives (with 3.6) | Program facts from official pages, structured, dated, linked | both | L | ongoing | Highest chance of misleading; most churn. |

### 8.2 Ongoing cost

Every shipped pack needs someone to watch its source. Rough yearly work once phases 0 to 4
exist: a day for payroll taxes and per diem, and for unions, a day or two per agreement
renewal plus checking the changed rows. That is the real price of this feature; if nobody
will do it, ship fewer packs rather than stale ones.

### 8.3 The data calendar

| When | What changes |
|---|---|
| January 1 | Social Security base, state unemployment wage bases and rates, many minimum wages, Canadian CPP and EI; IRS mileage (announced late December); the prior year's FUTA credit reductions on Form 940 Schedule A |
| April 6 | UK tax year (National Insurance thresholds) |
| July 1 | Some state minimum wages and fiscal-year state rates |
| Early August | IATSE Basic and Area Standards steps (Videotape steps late September or early October); GSA announces the next fiscal year |
| September 1 | California workers' comp advisory rates |
| October 1 | GSA federal fiscal year rates take effect; SSA announces next year's base |
| November | FUTA credit reductions final; CRA announces next year's CPP and EI |
| Monthly | State Department foreign per diem |
| Mid-2027 | IATSE Basic Agreement and Area Standards Agreement renewals: a large refresh wave |

## 9. Open questions for the owner

1. **Shipping threshold**: only rows a person verified against the public source? (The
   plan assumes yes.) Or also `cross_checked` cards, with a visible badge?
2. **Check-only material**: keep cross-checking against the purchased payroll guide and the
   production-held cards at all? Will you read the guide's purchase terms and say whether
   they allow it? If unclear, the plan drops the practice and uses public duplicates.
3. **Delivery**: bundle packs in app releases only (recommended), or also a separate data
   channel later? If a channel, on with the updater's automatic checks, or off until the
   user turns it on?
4. **Publishing the data**: keep `method-budget-data` private and ship only packs, or make
   the extracted figures public under an open licence (CC0 or CC BY 4.0)? Public data
   invites corrections but exposes the repository's history and docs to the scrub list.
5. **Where rate tables live**: inside budget files as a setup kind, or only in packs and
   the library (6.2)?
6. **What a fill writes**: a number with a line source stamp, or a global per
   classification (6.3)?
7. **Payroll taxes by default**: should the New project window add employer payroll tax
   fringes automatically, and which states first (the plan proposes CA, NY, GA, NM, LA,
   IL)?
8. **Workers' comp**: an empty placeholder fringe only (recommended), or state advisory
   figures where a bureau's terms allow it?
9. **Incentives**: ship anything before 3.6? If yes, links and a one-line summary only?
10. **Change watcher**: may a script make polite monthly checks of known document URLs on
    hosts that allow automation, or must every check be by hand?
11. **First wave**: LA, New York and Atlanta crew plus SAG-AFTRA, DGA and WGA 2026, as
    proposed? Or a different set (Chicago and New Mexico are routed too)?
12. **Who verifies**: you, the line producer who shared the library, a paid checker, or a
    mix? How are they credited (a handle, or not at all)?
13. **Asking the unions**: worth writing to the big unions and AMPTP to say what we use
    and offer corrections? Not needed legally; it may build goodwill or draw attention.
14. **Disclaimer wording and placement**: one line in each pack and the lookup, plus a
    longer note in the New project window and the website?
15. **International order**: after the US, which countries' payroll and per diem first
    (UK and Canada are the easiest by licence and by cards already extracted)?

## 10. Sources

All accessed 2026-10-12.

Payroll taxes:
- SSA, Contribution and benefit base: https://www.ssa.gov/oact/COLA/cbb.html
- IRS, Questions and answers for the Additional Medicare Tax:
  https://www.irs.gov/businesses/small-businesses-self-employed/questions-and-answers-for-the-additional-medicare-tax
- DOL ETA, Significant Provisions of State UI Laws, January 2026:
  https://oui.doleta.gov/unemploy/content/sigpros/2020-2029/January2026.pdf
- Federal Register, FUTA credit reduction notice (public inspection copy):
  https://public-inspection.federalregister.gov/2026-00342.pdf
- PayrollOrg, California and Virgin Islands face FUTA credit reduction for 2025:
  https://payroll.org/news-resources/news/news-detail/2025/11/11/california-virgin-islands-face-futa-credit-reduction-for-2025
- California EDD, Rates and withholding: https://edd.ca.gov/en/payroll_taxes/rates_and_withholding/
- Connecticut DOL, unemployment tax rate for 2026:
  https://portal.ct.gov/dol/divisions/unemployment-insurance-tax/information-on-unemployment-tax-rate-for-calendar-year-2026
- HMRC, Rates and thresholds for employers 2026 to 2027:
  https://www.gov.uk/guidance/rates-and-thresholds-for-employers-2026-to-2027
- Investment Executive, CRA announces CPP maximum pensionable earnings for 2026:
  https://www.investmentexecutive.com/news/cra-announces-cpp-maximum-pensionable-earnings-for-2026/

Per diem, mileage, wages:
- GSA, Per Diem API: https://open.gsa.gov/api/perdiem/
- GSA, Developer resources terms: https://gsa.gov/node/78901
- Federal Register, FY 2027 per diem (FR Doc. 2026-18439):
  https://public-inspection.federalregister.gov/2026-18439.pdf
- GSA, FY 2027 per diem highlights: https://www.gsa.gov/system/files/FY2027perdiemhighlights.docx
- State Department, Foreign per diem rates: https://allowances.state.gov/ForeignPerdiemRates.asp
- IRS, IR-2025-128, 2026 standard mileage rates:
  https://www.irs.gov/newsroom/irs-sets-2026-business-standard-mileage-rate-at-725-cents-per-mile-up-25-cents
- DOL WHD, State minimum wage laws: https://www.dol.gov/agencies/whd/minimum-wage/state
- BLS, API FAQs: https://www.bls.gov/developers/api_faqs.htm
- BLS, API terms of service: https://www.bls.gov/developers/termsOfService.htm

Unions and benefits:
- AMPTP home and contracts: https://amptp.org/ and https://amptp.org/contracts/
- AMPTP robots.txt: https://www.amptp.org/robots.txt
- DGA robots.txt: https://www.dga.org/robots.txt
- WGA robots.txt: https://www.wga.org/robots.txt
- SAG-AFTRA online policies: https://sagaftra.org/privacy-policy-terms-use (403 to
  automated fetch; also https://www.sagaftra.org/robots.txt and https://iatse.net/robots.txt)
- SAG-AFTRA Health Plan terms: https://sagaftraplans.org/node/411
- Variety, IATSE ratifies new three-year deal (2024):
  https://variety.com/2024/tv/news/iatse-ratifies-contract-amptp-1236068940
- Wrapbook, IATSE contract summary (increase dates, Videotape dates):
  https://www.wrapbook.com/blog/iatse-contract
- GreenSlate, East Coast MPIPHP rate increase 2026 (secondary, shows a mid-year change):
  https://greenslate.com/blog/east-coast-mpiphp-rates

Workers' compensation:
- Connecticut Insurance Department, NCCI filing disclaimer:
  https://portal.ct.gov/-/media/CID/NCCI-2020-Filing-Disclaimer.pdf?la=en
- Texas Department of Insurance, 2026 Texas loss costs (NCCI licensed product notice):
  https://tdi.texas.gov/WC/regulation/documents/2026txlosscosts.pdf
- California Department of Insurance, workers' comp decision and order, 10 July 2026:
  https://www.insurance.ca.gov/0400-news/0100-press-releases/2026/upload/Workers-Comp-Decision-and-Order-7-10-26.pdf

Incentives:
- Georgia Film Office, incentives and applications:
  https://www.georgia.org/industries/film-entertainment/georgia-film-tv-production/production-incentives
- California Film Commission, Program 4.0 guidelines:
  https://cdn.film.ca.gov/wp-content/uploads/2025/08/4.0-Program-Guidelines-1.pdf

Law:
- Feist Publications v. Rural Telephone Service, 499 U.S. 340 (1991):
  https://www.law.cornell.edu/supremecourt/text/499/340
- 17 U.S.C. 105: https://www.law.cornell.edu/uscode/text/17/105
- Georgia v. Public.Resource.Org (2020), summary:
  https://globalfreedomofexpression.columbia.edu/cases/georgia-v-public-resource-org-inc
- RFC 9309, Robots Exclusion Protocol: https://www.rfc-editor.org/rfc/rfc9309
- hiQ v. LinkedIn consent judgment, analysis:
  https://www.zwillgen.com/alternative-data/hiq-v-linkedin-wrapped-up-web-scraping-lessons-learned/
- Meta v. Bright Data (N.D. Cal. 2024), analysis: https://www.fbm.com/post/102kqw7/
- Directive 96/9/EC, chapter III (sui generis right):
  https://www.legislation.gov.uk/eudr/1996/9/chapter/III
- British Horseracing Board v. William Hill (C-203/02), commentary:
  https://cyberlaw.stanford.edu/blog/2005/02/european-court-justice-defines-scope-database-directive-protection-exclude-obtaining
- Spain, Texto refundido de la Ley de Propiedad Intelectual (BOE-A-1996-8930):
  https://www.boe.es/eli/es/rdlg/1996/04/12/1/con
