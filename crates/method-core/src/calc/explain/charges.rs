//! Explaining charges and credits.

use super::*;

impl<'a> Explainer<'a> {
    // -----------------------------------------------------------------------
    // Charges and credits

    /// Every costed line with where it is and its counted value.
    fn costed(&self) -> impl Iterator<Item = (usize, usize, &str, &str, &Detail, &RowCalc)> + '_ {
        self.b.categories().enumerate().flat_map(move |(ci, cat)| {
            cat.accounts.iter().enumerate().flat_map(move |(ai, acct)| {
                acct.details
                    .iter()
                    .zip(&self.accounts[ci][ai].rows)
                    .filter(|(d, _)| d.kind.is_costed())
                    .map(move |(d, r)| (ci, ai, cat.code.as_str(), acct.code.as_str(), d, r))
            })
        })
    }

    /// What a percent charge is a percent of: the grand total before
    /// charges.
    fn before_charges_total(&self) -> R<Cents> {
        let s = Step::sum("Before charges", "", self.before_charges_steps()?)?;
        s.amount.ok_or_else(ovf)
    }

    fn applied_charge(&self, i: usize) -> R<&crate::model::Charge> {
        self.b
            .charges
            .iter()
            .filter(|c| c.applied)
            .nth(i)
            .ok_or_else(|| gone("that charge"))
    }

    pub(super) fn charge_step(&self, i: usize) -> R<Step> {
        let c = self.applied_charge(i)?;
        let dropped = dropped_from(self.b);
        let mut excluded: Cents = 0;
        let mut notes = Vec::new();
        // Excluded lines, gathered per account with the rule that caught them.
        let mut per_account: BTreeMap<(usize, usize), (Cents, BTreeSet<String>)> = BTreeMap::new();
        for (ci, ai, cat, acct, d, r) in self.costed() {
            let mut why = BTreeSet::new();
            for g in &d.groups {
                if c.excluded_groups.contains(g) {
                    why.insert(format!("group {g}"));
                }
            }
            if c.excluded_accounts.contains(acct) {
                why.insert(format!("account {acct}"));
            }
            if c.excluded_categories.contains(cat) {
                why.insert(format!("category {cat}"));
            }
            if why.is_empty() {
                continue;
            }
            let v = if dropped.is_some_and(|d| ci >= d) {
                r.total
            } else {
                r.total.checked_add(r.fringe_total).ok_or_else(ovf)?
            };
            let e = per_account.entry((ci, ai)).or_insert((0, BTreeSet::new()));
            e.0 = e.0.checked_add(v).ok_or_else(ovf)?;
            e.1.extend(why);
        }
        let mut ex_children = Vec::new();
        for (desc, amt) in &c.excluded_amounts {
            excluded = excluded.checked_add(*amt).ok_or_else(ovf)?;
            ex_children.push(Step::note_amount(
                say!("engine.working.label.excluded_amount"),
                if desc.trim().is_empty() {
                    say!("engine.working.typed_in")
                } else {
                    Words::from(desc.trim())
                },
                *amt,
            ));
        }
        for ((ci, ai), (amt, why)) in &per_account {
            excluded = excluded.checked_add(*amt).ok_or_else(ovf)?;
            let what = if dropped.is_some_and(|d| *ci >= d) {
                "its lines (their fringes are left out of every total)"
            } else {
                "its lines and their fringes"
            };
            ex_children.push(
                Step::note_amount(
                    self.account_name(*ci, *ai),
                    format!(
                        "{what}, left out by {} (each line counted once)",
                        why.iter().cloned().collect::<Vec<_>>().join(", ")
                    ),
                    *amt,
                )
                .with_link(Link::Account { ci: *ci, ai: *ai }),
            );
        }
        let (amount, label_text) = match &c.kind {
            ChargeKind::Percent(p) => {
                let base = self.before_charges_total()?;
                notes.push(Step::note_amount(
                    say!("engine.working.label.base"),
                    say!("engine.working.charge_base"),
                    base,
                ));
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.excluded"),
                        say!("engine.working.taken_off_base"),
                        excluded,
                    )
                    .with(ex_children),
                );
                let net = base.checked_sub(excluded).ok_or_else(ovf)?;
                let arith = self.b.arithmetic;
                let exact = percent_of(arith, net, Num::of(arith, *p)).ok_or_else(ovf)?;
                let amount = exact.to_cents().ok_or_else(ovf)?;
                notes.push(Step::note(
                    say!("engine.working.label.charge"),
                    say!(
                        "engine.working.charge_percent_of",
                        percent = num(*p),
                        base = money(base),
                        excluded = money(excluded),
                        exact = dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, amount));
                (
                    amount,
                    say!("engine.working.percent_of_budget", percent = num(*p)),
                )
            }
            ChargeKind::Flat { amount, currency } => {
                let base = match currency {
                    Some(cur) if *cur != self.b.base_currency => {
                        let rate = self
                            .b
                            .currencies
                            .get(cur)
                            .ok_or_else(|| gone(&format!("currency {cur}")))?
                            .rate;
                        let arith = self.b.arithmetic;
                        let exact = Money::cents(arith, *amount)
                            .checked_mul(Num::of(arith, rate))
                            .ok_or_else(ovf)?;
                        let conv = exact.to_cents().ok_or_else(ovf)?;
                        notes.push(Step::note(
                            say!("engine.working.label.fee"),
                            say!(
                                "engine.working.fee_converted",
                                amount = money(*amount),
                                currency = cur,
                                rate = num(rate),
                                base = self.b.base_currency,
                                exact = dmoney(exact)
                            ),
                        ));
                        notes.extend(rounding(exact, conv));
                        conv
                    }
                    _ => {
                        notes.push(Step::note_amount(
                            say!("engine.working.label.fee"),
                            say!("engine.working.a_flat_fee"),
                            *amount,
                        ));
                        *amount
                    }
                };
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.excluded"),
                        say!("engine.working.taken_off_fee"),
                        excluded,
                    )
                    .with(ex_children),
                );
                let amount = base.checked_sub(excluded).ok_or_else(ovf)?;
                notes.push(Step::note(
                    say!("engine.working.label.charge"),
                    format!("{} - {} = {}", money(base), money(excluded), money(amount)),
                ));
                (amount, say!("engine.working.a_flat_fee"))
            }
        };
        Ok(Step::value(&c.name, label_text, amount)
            .with(notes)
            .with_expand(Target::Charge(i)))
    }

    pub(super) fn charge(&self, i: usize) -> R<(Words, Step)> {
        let c = self.applied_charge(i)?;
        let mut s = self.charge_step(i)?;
        s.expand = None;
        Ok((say!("engine.working.title.charge", name = c.name), s))
    }

    fn applied_credit(&self, i: usize) -> R<&crate::model::Credit> {
        self.b
            .credits
            .iter()
            .filter(|c| c.applied)
            .nth(i)
            .ok_or_else(|| gone("that credit"))
    }

    pub(super) fn credit_step(&self, i: usize) -> R<Step> {
        let c = self.applied_credit(i)?;
        let arith = self.b.arithmetic;
        let mut notes = Vec::new();
        let excluded_fringes = &c.excluded_fringes;
        // Only lines at the credit's locations count (decisions.md, entry 80).
        let here = |d: &Detail| counts_at(&c.locations, d.location.as_deref());
        let mut total = Money::cents(arith, 0);
        if !c.locations.is_empty() {
            let names = c.locations.iter().cloned().collect::<Vec<_>>();
            notes.push(Step::note(
                say!("engine.working.label.locations"),
                say!("engine.working.only_lines_at", locations = names.join(", ")),
            ));
        }
        for (k, cr) in c.rows.iter().enumerate() {
            let mut row_notes = Vec::new();
            let (gross, excluded) = match &cr.source {
                CreditSource::Flat(g) => {
                    row_notes.push(Step::note_amount(
                        say!("engine.working.label.gross"),
                        say!("engine.working.typed_in"),
                        *g,
                    ));
                    (*g, 0)
                }
                CreditSource::Group { group } => {
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    let mut per_account: BTreeMap<(usize, usize), (Cents, Cents)> = BTreeMap::new();
                    for (ci, ai, _, _, d, r) in self.costed() {
                        if !r.included || !d.groups.contains(group) || !here(d) {
                            continue;
                        }
                        let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                        gross = gross.checked_add(v).ok_or_else(ovf)?;
                        let mut e: Cents = 0;
                        for (name, s) in &r.fringes {
                            if excluded_fringes.contains(name) {
                                e = e.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                            }
                        }
                        ex = ex.checked_add(e).ok_or_else(ovf)?;
                        let slot = per_account.entry((ci, ai)).or_insert((0, 0));
                        slot.0 = slot.0.checked_add(v).ok_or_else(ovf)?;
                        slot.1 = slot.1.checked_add(e).ok_or_else(ovf)?;
                    }
                    let lines = per_account
                        .iter()
                        .map(|(&(ci, ai), &(v, _))| {
                            Step::note_amount(
                                self.account_name(ci, ai),
                                say!("engine.working.counted_lines_tagged", group = group),
                                v,
                            )
                            .with_link(Link::Account { ci, ai })
                        })
                        .collect();
                    row_notes.push(
                        Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.lines_tagged", group = group),
                            gross,
                        )
                        .with(lines),
                    );
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
                CreditSource::Category { category } => {
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    let mut found = None;
                    for (ci, _, cat, _, d, r) in self.costed() {
                        if cat != category {
                            continue;
                        }
                        found = Some(ci);
                        if !here(d) {
                            continue;
                        }
                        let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                        gross = gross.checked_add(v).ok_or_else(ovf)?;
                        for (name, s) in &r.fringes {
                            if excluded_fringes.contains(name) {
                                ex = ex.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                            }
                        }
                    }
                    if found.is_none() {
                        found = self.b.categories().position(|c| c.code == *category);
                    }
                    let gross_note = match found {
                        Some(ci) => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!(
                                "engine.working.gross_category",
                                category = self.category_name(ci)
                            ),
                            gross,
                        )
                        .with_link(Link::Category { ci }),
                        None => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.gross_no_category", category = category),
                            gross,
                        ),
                    };
                    row_notes.push(gross_note);
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
                CreditSource::Account { account } => {
                    // The first account with the number, as the engine
                    // takes it.
                    let found = self.b.categories().enumerate().find_map(|(ci, cat)| {
                        cat.accounts
                            .iter()
                            .position(|a| a.code == *account)
                            .map(|ai| (ci, ai))
                    });
                    let mut gross: Cents = 0;
                    let mut ex: Cents = 0;
                    if let Some(at) = found {
                        for (ci, ai, _, _, d, r) in self.costed() {
                            if (ci, ai) != at || !here(d) {
                                continue;
                            }
                            let v = r.total.checked_add(r.fringe_total).ok_or_else(ovf)?;
                            gross = gross.checked_add(v).ok_or_else(ovf)?;
                            for (name, s) in &r.fringes {
                                if excluded_fringes.contains(name) {
                                    ex = ex.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
                                }
                            }
                        }
                    }
                    let gross_note = match found {
                        Some((ci, ai)) => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!(
                                "engine.working.gross_account",
                                account = self.account_name(ci, ai)
                            ),
                            gross,
                        )
                        .with_link(Link::Account { ci, ai }),
                        None => Step::note_amount(
                            say!("engine.working.label.gross"),
                            say!("engine.working.gross_no_account", account = account),
                            gross,
                        ),
                    };
                    row_notes.push(gross_note);
                    if !excluded_fringes.is_empty() {
                        let names = excluded_fringes.iter().cloned().collect::<Vec<_>>();
                        row_notes.push(Step::note_amount(
                            say!("engine.working.label.excluded_fringes"),
                            names.join(", "),
                            ex,
                        ));
                    }
                    (gross, ex)
                }
            };
            let net = gross.checked_sub(excluded).ok_or_else(ovf)?;
            row_notes.push(Step::note_amount(
                say!("engine.working.label.net_spend"),
                format!("{} - {}", money(gross), money(excluded)),
                net,
            ));
            // The engine's own unrounded working, so the amounts agree.
            let (credit, _) = credit_row(arith, net, cr.percent, cr.cap).ok_or_else(ovf)?;
            let pct = |v: Money| {
                v.checked_mul(Num::of(arith, cr.percent))
                    .map(Money::percent)
            };
            let net_x = Money::cents(arith, net);
            match cr.cap {
                CreditCap::None => {
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of",
                            percent = num(cr.percent),
                            base = money(net),
                            exact = dmoney(credit)
                        ),
                    ));
                }
                CreditCap::QualifiedSpend(cap) => {
                    let spend = net_x
                        .checked_min(Money::cents(arith, cap))
                        .ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        say!("engine.working.label.cap_on_spend"),
                        say!(
                            "engine.working.cap_on_spend",
                            cap = money(cap),
                            spend = dmoney(spend)
                        ),
                    ));
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of",
                            percent = num(cr.percent),
                            base = dmoney(spend),
                            exact = dmoney(credit)
                        ),
                    ));
                }
                CreditCap::Credit(cap) => {
                    let uncapped = pct(net_x).ok_or_else(ovf)?;
                    row_notes.push(Step::note(
                        say!("engine.working.label.credit"),
                        say!(
                            "engine.working.percent_of_capped",
                            percent = num(cr.percent),
                            base = money(net),
                            exact = dmoney(uncapped),
                            cap = money(cap),
                            credit = dmoney(credit)
                        ),
                    ));
                }
            }
            row_notes.push(Step::note(
                say!("engine.working.label.not_rounded_yet"),
                say!("engine.working.rounded_once"),
            ));
            total = total.checked_add(credit).ok_or_else(ovf)?;
            let label = if cr.description.trim().is_empty() {
                say!("engine.working.label.row_n", row = k + 1)
            } else {
                Words::from(cr.description.trim())
            };
            notes.push(Step::note(label, dmoney(credit)).with(row_notes));
        }
        notes.push(Step::note(
            say!("engine.working.label.rows_added"),
            dmoney(total),
        ));
        // The engine's own figures for what comes after the rows
        // (decisions.md, entry 88): the minimum, the shared cap and the fee.
        let reported = self
            .report
            .credits
            .get(i)
            .ok_or_else(|| gone("that credit"))?;
        let mut capped = credit_cap(arith, total, c.cap).ok_or_else(ovf)?;
        if let Some(min) = c.minimum_spend {
            let mut qualified: Cents = 0;
            for r in &reported.rows {
                qualified = qualified.checked_add(r.net).ok_or_else(ovf)?;
            }
            if reported.below_minimum {
                notes.push(Step::note(
                    say!("engine.working.label.minimum_spend"),
                    say!(
                        "engine.working.below_minimum",
                        spend = money(qualified),
                        minimum = money(min)
                    ),
                ));
                capped = Money::cents(arith, 0);
            } else {
                notes.push(Step::note(
                    say!("engine.working.label.minimum_spend"),
                    say!(
                        "engine.working.at_least_minimum",
                        spend = money(qualified),
                        minimum = money(min)
                    ),
                ));
            }
        }
        if let Some(cap) = c.cap.filter(|_| !reported.below_minimum) {
            notes.push(Step::note(
                say!("engine.working.label.credit_cap"),
                say!(
                    "engine.working.at_most",
                    cap = money(cap),
                    capped = dmoney(capped)
                ),
            ));
        }
        if let Some(name) = c.shared_cap.as_deref() {
            let amount = self.b.shared_caps.get(name);
            match (amount, reported.shared) {
                (Some(cap), Some(share)) => {
                    let others: Vec<&str> = self
                        .b
                        .credits
                        .iter()
                        .filter(|o| o.applied && o.name != c.name)
                        .filter(|o| o.shared_cap.as_deref() == Some(name))
                        .map(|o| o.name.as_str())
                        .collect();
                    notes.push(Step::note(
                        say!("engine.working.label.shared_cap"),
                        say!(
                            "engine.working.shared_cap_share",
                            name = quoted(name),
                            others = others.join(", "),
                            cap = money(*cap),
                            capped = dmoney(capped),
                            share = money(share)
                        ),
                    ));
                    capped = Money::cents(arith, share);
                }
                (Some(cap), None) => notes.push(Step::note(
                    say!("engine.working.label.shared_cap"),
                    say!(
                        "engine.working.shared_cap_within",
                        name = quoted(name),
                        cap = money(*cap)
                    ),
                )),
                (None, _) => notes.push(Step::note(
                    say!("engine.working.label.shared_cap"),
                    say!("engine.working.shared_cap_missing", name = quoted(name)),
                )),
            }
        }
        let keep = credit_keep(arith, c.discount_percent).ok_or_else(ovf)?;
        let exact = capped.checked_mul_money(keep).ok_or_else(ovf)?;
        if !c.discount_percent.is_zero() {
            notes.push(Step::note(
                say!("engine.working.label.discount"),
                say!(
                    "engine.working.discount",
                    percent = num(c.discount_percent),
                    capped = dmoney(capped),
                    keep = keep,
                    exact = dmoney(exact)
                ),
            ));
        }
        let rounded = exact.to_cents().ok_or_else(ovf)?;
        notes.extend(rounding(exact, rounded));
        let amount = rounded.checked_sub(reported.fee).ok_or_else(ovf)?;
        if c.broker_fee != 0 {
            let text = if reported.fee == c.broker_fee {
                say!(
                    "engine.working.fee_less",
                    fee = money(c.broker_fee),
                    amount = money(amount)
                )
            } else {
                say!(
                    "engine.working.fee_less_floor",
                    fee = money(c.broker_fee),
                    taken = money(reported.fee),
                    amount = money(amount)
                )
            };
            notes.push(Step::note(say!("engine.working.label.brokers_fee"), text));
        }
        Ok(
            Step::value(&c.name, say!("engine.working.applied_credit"), amount)
                .with(notes)
                .with_expand(Target::Credit(i)),
        )
    }

    pub(super) fn credit(&self, i: usize) -> R<(Words, Step)> {
        let c = self.applied_credit(i)?;
        let mut s = self.credit_step(i)?;
        s.expand = None;
        Ok((say!("engine.working.title.credit", name = c.name), s))
    }
}
