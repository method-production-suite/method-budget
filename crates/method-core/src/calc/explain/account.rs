//! Working out one account: its lines, fringes and persons.

use super::*;

// ---------------------------------------------------------------------------
// Working out one account

/// A unit name with its plural when the quantity isn't one.
fn unit_words(b: &Budget, unit: &str, qty: Dec) -> String {
    let u = unit.trim();
    if u.is_empty() {
        return String::new();
    }
    match b.units.get(u) {
        Some(def) if qty != Dec::ONE && !def.plural.trim().is_empty() => {
            format!(" {}", def.plural.trim())
        }
        _ => format!(" {u}"),
    }
}

/// A number as the working shows it: a decimal, or a double with the
/// shortest digits that give it back (`0.3333333333333333`).
fn nwords(n: Num) -> String {
    match n {
        Num::Dec(d) => num(d),
        Num::F64(v) => {
            // Grouped like a decimal.
            let t = float_text(v);
            let (sign, t) = match t.strip_prefix('-') {
                Some(r) => ("-", r),
                None => ("", t.as_str()),
            };
            match t.split_once('.') {
                Some((i, f)) => format!("{sign}{}.{f}", group(i)),
                None => format!("{sign}{}", group(t)),
            }
        }
    }
}

/// A formula and its value: `40` or `SHOOT_DAYS + PREP_DAYS = 40
/// (SHOOT_DAYS = 30, PREP_DAYS = 10)`.
fn formula_words(ctx: &Ctx, f: &Formula, value: Num) -> String {
    let src = f.0.trim();
    if let Ok(v) = src.parse::<Dec>() {
        if ctx.num(v) == value {
            return nwords(value);
        }
    }
    // A number with more places than a decimal keeps (Movie Magic only).
    let plain = !src.is_empty() && src.bytes().all(|c| c.is_ascii_digit() || c == b'.');
    if plain && matches!(value, Num::F64(v) if src.parse::<f64>() == Ok(v)) {
        return src.to_string();
    }
    let mut vars = BTreeSet::new();
    if let Ok(e) = Expr::parse(src) {
        e.vars(&mut vars);
    }
    let globals: Vec<String> = vars
        .iter()
        .filter_map(|n| ctx.globals.get(n).map(|v| format!("{n} = {}", nwords(*v))))
        .collect();
    if globals.is_empty() {
        format!("{src} = {}", nwords(value))
    } else if globals.len() == 1 && vars.len() == 1 && vars.contains(src) {
        format!("global {}", globals[0])
    } else {
        format!("{src} = {} ({})", nwords(value), globals.join(", "))
    }
}

/// Why a line is left out, in words, and what it would be counted
/// (`total`): "...; counted, it would be 1,200.00".
fn left_out(ctx: &Ctx, d: &Detail, total: Cents) -> Words {
    let total = money(total);
    let rules = ctx.b.group_rules;
    let counted = d
        .groups
        .iter()
        .map(|g| ctx.b.groups.get(g).is_none_or(|x| x.in_budget_total));
    match rules.counts(counted) {
        Ok(()) => {}
        Err(LeftOut::Ungrouped) => {
            return if d.kind == RowKind::Adjustment {
                say!(
                    "engine.working.left_out_adjustment_ungrouped",
                    total = total
                )
            } else {
                say!("engine.working.left_out_ungrouped", total = total)
            };
        }
        Err(LeftOut::GroupOff) => {
            let off: Vec<&str> = d
                .groups
                .iter()
                .filter(|g| ctx.b.groups.get(*g).is_some_and(|x| !x.in_budget_total))
                .map(String::as_str)
                .collect();
            let groups = off.join(", ");
            let conflict = off.len() < d.groups.len();
            return match (off.len() == 1, conflict) {
                (true, false) => say!(
                    "engine.working.left_out_group_off",
                    groups = groups,
                    total = total
                ),
                (false, false) => say!(
                    "engine.working.left_out_groups_off",
                    groups = groups,
                    total = total
                ),
                (true, true) => say!(
                    "engine.working.left_out_group_off_conflict",
                    groups = groups,
                    total = total
                ),
                (false, true) => say!(
                    "engine.working.left_out_groups_off_conflict",
                    groups = groups,
                    total = total
                ),
            };
        }
    }
    let ex: Vec<&str> = d
        .groups
        .iter()
        .filter(|g| ctx.opts.exclude_groups.contains(*g))
        .map(String::as_str)
        .collect();
    if !ex.is_empty() {
        return say!(
            "engine.working.left_out_by_report",
            groups = ex.join(", "),
            total = total
        );
    }
    say!("engine.working.left_out_not_shown", total = total)
}

pub(super) fn account_calc(
    ctx: &Ctx,
    ci: usize,
    ai: usize,
    code: &str,
    details: &[Detail],
) -> R<AccountCalc> {
    let base = &ctx.b.base_currency;
    let mut rows: Vec<RowCalc> = Vec::with_capacity(details.len());
    let mut since_adjustment = Window::default();
    let mut adj_rows: Vec<usize> = Vec::new();
    let mut sub_rows: Vec<usize> = Vec::new();
    let zero = Factors {
        amount: ctx.num(Dec::ZERO),
        x: ctx.num(Dec::ONE),
        x4: ctx.num(Dec::ONE),
        quantity: ctx.num(Dec::ZERO),
    };
    let mut factors: Vec<Factors> = Vec::with_capacity(details.len());
    let blank_at = || String::new();

    for (i, d) in details.iter().enumerate() {
        let link = Link::Row { ci, ai, row: d.id };
        match d.kind {
            RowKind::Subtotal => {
                rows.push(RowCalc {
                    kind: d.kind,
                    included: true,
                    total: 0,
                    value: None,
                    adds: std::mem::take(&mut sub_rows),
                    fringes: Vec::new(),
                    fringe_total: 0,
                });
                factors.push(zero);
                continue;
            }
            RowKind::Separator => {
                rows.push(RowCalc {
                    kind: d.kind,
                    included: true,
                    total: 0,
                    value: None,
                    adds: Vec::new(),
                    fringes: Vec::new(),
                    fringe_total: 0,
                });
                factors.push(zero);
                continue;
            }
            RowKind::Line | RowKind::Adjustment => {}
        }

        let amount = ctx.eval(&d.amount, Dec::ZERO, &blank_at)?;
        let x = ctx.eval(&d.x, Dec::ONE, &blank_at)?;
        let x4 = ctx.eval(&d.x4, Dec::ONE, &blank_at)?;
        let quantity = Factors::quantity(amount, x, x4).ok_or_else(ovf)?;
        let shown = |n: Num| n.shown().ok_or_else(ovf);
        let included = ctx.includes(d, &blank_at)?;
        let mut notes = Vec::new();

        // Amount, X and 4X.
        let amount_text = if d.amount.is_blank() {
            say!("engine.working.blank_counts_as_0")
        } else {
            Words::from(format!(
                "{}{}",
                formula_words(ctx, &d.amount, amount),
                unit_words(ctx.b, &d.unit, shown(amount)?)
            ))
        };
        notes.push(Step::note(say!("engine.working.label.amount"), amount_text));
        if !d.x.is_blank() {
            notes.push(Step::note(
                say!("engine.working.label.x"),
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x, x),
                    unit_words(ctx.b, &d.unit2, shown(x)?)
                ),
            ));
        }
        if !d.x4.is_blank() {
            notes.push(Step::note(
                say!("engine.working.label.x4"),
                format!(
                    "{}{}",
                    formula_words(ctx, &d.x4, x4),
                    unit_words(ctx.b, &d.unit4, shown(x4)?)
                ),
            ));
        }
        if !d.x.is_blank() || !d.x4.is_blank() {
            let mut parts = vec![nwords(amount)];
            if !d.x.is_blank() {
                parts.push(nwords(x));
            }
            if !d.x4.is_blank() {
                parts.push(nwords(x4));
            }
            notes.push(Step::note(
                say!("engine.working.label.quantity"),
                format!("{} = {}", parts.join(" x "), nwords(quantity)),
            ));
        }

        let total = match d.kind {
            RowKind::Adjustment => {
                let exact = since_adjustment
                    .money(ctx.arith)
                    .checked_mul(quantity)
                    .ok_or_else(ovf)?;
                let t = exact.to_cents().ok_or_else(ovf)?;
                let pct = shown(quantity)?
                    .checked_mul(Dec::from_int(100))
                    .ok_or_else(ovf)?;
                let window: Vec<Step> = adj_rows
                    .iter()
                    .map(|&k| {
                        let row = details[k].id;
                        Step::note_amount(line_label(k, &details[k]), "", rows[k].total)
                            .with_link(Link::Row { ci, ai, row })
                    })
                    .collect();
                let base_text = match ctx.arith {
                    Arithmetic::Exact => say!("engine.working.adjustment_base"),
                    Arithmetic::Mmb => say!("engine.working.adjustment_base_doubles"),
                };
                notes.push(
                    Step::note_amount(
                        say!("engine.working.label.base"),
                        base_text,
                        since_adjustment.cents,
                    )
                    .with(window),
                );
                let base_words = match ctx.arith {
                    Arithmetic::Exact => money(since_adjustment.cents),
                    Arithmetic::Mmb => dmoney(since_adjustment.money(ctx.arith)),
                };
                notes.push(Step::note(
                    say!("engine.working.label.adjustment"),
                    format!(
                        "{base_words} x {} ({}%) = {}",
                        nwords(quantity),
                        num(pct),
                        dmoney(exact)
                    ),
                ));
                notes.extend(rounding(exact, t));
                t
            }
            _ => {
                let rate = ctx.eval(&d.rate, Dec::ZERO, &blank_at)?;
                let rate_text = if d.rate.is_blank() {
                    say!("engine.working.blank_counts_as_0")
                } else {
                    Words::from(format!(
                        "{}{}",
                        formula_words(ctx, &d.rate, rate),
                        unit_words(ctx.b, &d.unit3, Dec::ONE)
                    ))
                };
                notes.push(Step::note(say!("engine.working.label.rate"), rate_text));
                // Where the rate came from, when it was filled from a rate
                // table (decisions.md, entry 90): words only.
                if let Some(words) = crate::rates::source_words(ctx.b, d) {
                    notes.push(Step::note(say!("engine.working.label.rate_source"), words));
                }
                let line = Money::num(quantity).checked_mul(rate).ok_or_else(ovf)?;
                let local = line.to_cents().ok_or_else(ovf)?;
                let cur_code = match &d.currency {
                    Some(c) if c != base => Some(c),
                    _ => None,
                };
                let cur_words = cur_code.map(|c| format!(" {c}")).unwrap_or_default();
                notes.push(Step::note(
                    say!("engine.working.label.line"),
                    format!(
                        "{} x {} = {}{cur_words}",
                        nwords(quantity),
                        nwords(rate),
                        dmoney(line)
                    ),
                ));
                // Movie Magic converts the unrounded amount: the line rounds
                // once, in the base currency.
                let rounds_here = cur_code.is_none() || ctx.arith == Arithmetic::Exact;
                if rounds_here {
                    notes.extend(rounding(line, local));
                }
                match cur_code {
                    Some(c) => {
                        let cur = ctx
                            .b
                            .currencies
                            .get(c)
                            .ok_or_else(|| gone(&format!("currency {c}")))?;
                        let exact = converted(ctx.arith, line, local, cur.rate).ok_or_else(ovf)?;
                        let t = exact.to_cents().ok_or_else(ovf)?;
                        let text = if rounds_here || line.is_cents(local) {
                            say!(
                                "engine.working.currency",
                                amount = money(local),
                                currency = c,
                                rate = num(cur.rate),
                                base = base,
                                exact = dmoney(exact)
                            )
                        } else {
                            say!(
                                "engine.working.currency_unrounded",
                                amount = dmoney(line),
                                currency = c,
                                rate = num(cur.rate),
                                base = base,
                                exact = dmoney(exact)
                            )
                        };
                        notes.push(Step::note(say!("engine.working.label.currency"), text));
                        notes.extend(rounding(exact, t));
                        t
                    }
                    None => local,
                }
            }
        };
        let total_counted = if included { total } else { 0 };
        if !included {
            notes.push(Step::note_amount(
                say!("engine.working.label.left_out"),
                left_out(ctx, d, total),
                0,
            ));
        }
        if d.kind == RowKind::Adjustment && included {
            since_adjustment.clear();
            adj_rows.clear();
        } else {
            since_adjustment.add(total_counted).ok_or_else(ovf)?;
            adj_rows.push(i);
        }
        sub_rows.push(i);

        let text = if d.kind == RowKind::Adjustment {
            say!("engine.working.row_adjustment", code = code, line = i + 1)
        } else if included {
            say!("engine.working.row_line", code = code, line = i + 1)
        } else {
            say!(
                "engine.working.row_line_left_out",
                code = code,
                line = i + 1
            )
        };
        let value = Step::value(line_label(i, d), text, total_counted)
            .with(notes)
            .with_link(link)
            .with_expand(Target::Line(d.id));
        factors.push(Factors {
            amount,
            x,
            x4,
            quantity,
        });
        rows.push(RowCalc {
            kind: d.kind,
            included,
            total: total_counted,
            value: Some(value),
            adds: Vec::new(),
            fringes: Vec::new(),
            fringe_total: 0,
        });
    }

    fringes_calc(ctx, ci, ai, details, &mut rows, &factors)?;

    let fringe_total = rows
        .iter()
        .try_fold(0, |a: Cents, r| a.checked_add(r.fringe_total))
        .ok_or_else(ovf)?;
    Ok(AccountCalc { rows, fringe_total })
}

/// Fringe value steps for every line of one account (the engine's stage 2,
/// worked out again with its working shown).
fn fringes_calc(
    ctx: &Ctx,
    ci: usize,
    ai: usize,
    details: &[Detail],
    rows: &mut [RowCalc],
    factors: &[Factors],
) -> R<()> {
    let blank_at = || String::new();
    // Rates, once per fringe the lines carry.
    let mut rates: BTreeMap<&str, Num> = BTreeMap::new();
    for d in details {
        if !d.kind.is_costed() {
            continue;
        }
        for name in &d.fringes {
            if rates.contains_key(name.as_str()) {
                continue;
            }
            let f = ctx
                .b
                .fringes
                .get(name)
                .ok_or_else(|| gone(&format!("fringe {name}")))?;
            rates.insert(name.as_str(), ctx.eval(&f.rate, Dec::ZERO, &blank_at)?);
        }
    }
    // A fringe's rate when it is a formula: " (rate SAG_PCT = 18.5)".
    let rate_formula = |name: &str, rate: Num| -> Option<String> {
        let f = &ctx.b.fringes[name];
        let src = f.rate.0.trim();
        if src.parse::<Dec>().is_ok() {
            None
        } else {
            Some(formula_words(ctx, &f.rate, rate))
        }
    };
    let earns: Vec<bool> = details
        .iter()
        .zip(rows.iter())
        .map(|(d, r)| d.kind.is_costed() && r.included)
        .collect();
    // The engine's earners, from a row list that has what it reads.
    let shadow: Vec<RowTotals> = rows
        .iter()
        .map(|r| RowTotals {
            id: 0,
            kind: r.kind,
            included: r.included,
            quantity: Dec::ZERO,
            currency: String::new(),
            local_total: 0,
            total: r.total,
            subtotal: None,
            fringes: BTreeMap::new(),
            fringe_total: 0,
            fringe_full_total: 0,
        })
        .collect();
    let mut found: Vec<BTreeMap<String, Step>> = vec![BTreeMap::new(); details.len()];

    for earner in earners(details, &shadow) {
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for &i in &earner {
            if earns[i] {
                names.extend(details[i].fringes.iter().map(String::as_str));
            }
        }
        // Every row of an earner from a range shares its number.
        let range = details[earner[0]].fringe_range;
        for name in names {
            let f = &ctx.b.fringes[name];
            let FringeKind::Percent { cutoff: typed } = f.kind else {
                continue;
            };
            // Times the budget's cutoff factor, as the engine applies it.
            let cutoff = factored_cutoff(ctx.arith, typed, ctx.b.cutoff_factor).ok_or_else(ovf)?;
            let rate = rates[name];
            let lines: Vec<usize> = earner
                .iter()
                .copied()
                .filter(|&i| earns[i] && details[i].fringes.contains(name))
                .collect();
            // The cutoff, used up in row order, as the engine uses it.
            let mut left = cutoff;
            for (k, &i) in lines.iter().enumerate() {
                let total = rows[i].total;
                let before = left;
                let (taken, exact) =
                    percent_step(ctx.arith, rate, &mut left, total).ok_or_else(ovf)?;
                let amount = exact.to_cents().ok_or_else(ovf)?;
                let mut notes = Vec::new();
                if let Some(n) = range {
                    let others: Vec<String> = lines.iter().map(|&j| (j + 1).to_string()).collect();
                    notes.push(Step::note(
                        say!("engine.working.label.fringe_range"),
                        say!(
                            "engine.working.fringe_range",
                            range = n,
                            lines = others.join(", ")
                        ),
                    ));
                }
                notes.push(Step::note_amount(
                    say!("engine.working.label.earnings"),
                    say!("engine.working.this_lines_total"),
                    total,
                ));
                if let (Some(t), Some(c)) = (typed, cutoff) {
                    if ctx.b.cutoff_factor != CutoffFactor::One {
                        notes.push(cutoff_factor_note(ctx.b.cutoff_factor, t, c));
                    }
                }
                if let (Some(c), Some(l)) = (cutoff, before) {
                    let cutoff = money(c);
                    let left = money(l.max(0));
                    if l <= 0 && k > 0 {
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_used_up"),
                            say!(
                                "engine.working.cutoff_used_by_lines_above",
                                fringe = name,
                                cutoff = cutoff
                            ),
                            0,
                        ));
                    } else if total > l || l <= 0 {
                        let text = if k == 0 {
                            say!(
                                "engine.working.stops_per_earner",
                                fringe = name,
                                cutoff = cutoff
                            )
                        } else {
                            say!(
                                "engine.working.stops_per_earner_left",
                                fringe = name,
                                cutoff = cutoff,
                                left = left
                            )
                        };
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_reached"),
                            text,
                            taken,
                        ));
                    } else {
                        let text = if k == 0 {
                            say!("engine.working.per_earner_not_reached", cutoff = cutoff)
                        } else {
                            say!(
                                "engine.working.per_earner_left_not_reached",
                                cutoff = cutoff,
                                left = left
                            )
                        };
                        notes.push(Step::note(say!("engine.working.label.cutoff"), text));
                    }
                }
                notes.push(Step::note(
                    say!("engine.working.label.fringe"),
                    percent_words(rate, money(taken), exact, rate_formula(name, rate)),
                ));
                notes.extend(rounding(exact, amount));
                let kind_text = match cutoff {
                    Some(c) => say!(
                        "engine.working.percent_with_cutoff",
                        percent = nwords(rate),
                        cutoff = money(c)
                    ),
                    None => Words::from(format!("{}%", nwords(rate))),
                };
                let s = Step::value(name, kind_text, amount)
                    .with(notes)
                    .with_link(Link::Row {
                        ci,
                        ai,
                        row: details[i].id,
                    })
                    .with_expand(Target::LineFringe(details[i].id, name.to_string()));
                found[i].insert(name.to_string(), s);
            }
        }
    }

    // Flat fringes, per row.
    for (i, d) in details.iter().enumerate() {
        if !earns[i] {
            continue;
        }
        for name in &d.fringes {
            let f = &ctx.b.fringes[name];
            let FringeKind::Flat { unit, .. } = &f.kind else {
                continue;
            };
            let rate = rates[name.as_str()];
            // Counted as the engine counts, rounded once from the unrounded
            // amount; an exact division that need not end shows to 9 places.
            let c = flat_count(ctx.arith, &ctx.b.units, &f.kind, d, factors[i]).ok_or_else(ovf)?;
            let amount = c.amount(rate).ok_or_else(ovf)?;
            let exact = c.exact_amount(rate).ok_or_else(ovf)?;
            let (mut notes, text) = flat_words(ctx.b, name, d, unit.as_deref(), &c, rate, exact)?;
            if let (Some(n), Some(formula)) = (notes.last_mut(), rate_formula(name, rate)) {
                // The flat fringe's working is not catalogued (`text_said`
                // stays `None`), so its English can grow.
                debug_assert!(n.text_said.is_none());
                n.text.push_str(&format!(" (rate {formula})"));
            }
            notes.extend(rounding(exact, amount));
            let s = Step::value(name.as_str(), text, amount)
                .with(notes)
                .with_link(Link::Row { ci, ai, row: d.id })
                .with_expand(Target::LineFringe(d.id, name.clone()));
            found[i].insert(name.clone(), s);
        }
    }

    for (i, map) in found.into_iter().enumerate() {
        let mut total: Cents = 0;
        for s in map.values() {
            total = total.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
        }
        rows[i].fringe_total = total;
        rows[i].fringes = map.into_iter().collect();
    }
    Ok(())
}

/// "2100", "2100 and 2200", "2100, 2200 and 3100".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Percent fringe steps for every row with a person (the engine's person
/// pass, decisions.md, entry 79, worked out again with its working shown),
/// across every account, with the rows' and accounts' fringe totals added
/// up again.
pub(super) fn persons_calc(ctx: &Ctx, accounts: &mut [Vec<AccountCalc>]) -> R<()> {
    // Each person's earning rows, as (ci, ai, row), in budget order.
    // By the matching form of the name; shown as the first row spells it.
    let mut people: BTreeMap<String, Vec<(usize, usize, usize)>> = BTreeMap::new();
    let mut shown: BTreeMap<String, &str> = BTreeMap::new();
    for (ci, cat) in ctx.b.categories().enumerate() {
        for (ai, acct) in cat.accounts.iter().enumerate() {
            for (i, d) in acct.details.iter().enumerate() {
                let Some(name) = d.person_name().filter(|_| d.kind.is_costed()) else {
                    continue;
                };
                let key = persons::person_key(name);
                shown.entry(key.clone()).or_insert(name);
                if accounts[ci][ai].rows.get(i).is_some_and(|r| r.included) {
                    people.entry(key).or_default().push((ci, ai, i));
                }
            }
        }
    }
    let blank_at = || String::new();
    let mut found: BTreeMap<(usize, usize, usize), BTreeMap<String, Step>> = BTreeMap::new();
    let detail = |ci: usize, ai: usize, i: usize| -> R<&Detail> {
        ctx.b
            .categories()
            .nth(ci)
            .and_then(|c| c.accounts.get(ai))
            .and_then(|a| a.details.get(i))
            .ok_or_else(|| gone("row"))
    };
    let code = |ci: usize, ai: usize| -> String {
        ctx.b
            .categories()
            .nth(ci)
            .and_then(|c| c.accounts.get(ai))
            .map(|a| a.code.clone())
            .unwrap_or_default()
    };
    for (key, at) in &people {
        let person = shown.get(key).copied().unwrap_or(key.as_str());
        let mut names: BTreeSet<&str> = BTreeSet::new();
        for &(ci, ai, i) in at {
            names.extend(detail(ci, ai, i)?.fringes.iter().map(String::as_str));
        }
        for name in names {
            let Some(f) = ctx.b.fringes.get(name) else {
                continue;
            };
            let FringeKind::Percent { cutoff: typed } = f.kind else {
                continue;
            };
            let cutoff = factored_cutoff(ctx.arith, typed, ctx.b.cutoff_factor).ok_or_else(ovf)?;
            let rate = ctx.eval(&f.rate, Dec::ZERO, &blank_at)?;
            let mut lines = Vec::new();
            for &(ci, ai, i) in at {
                let d = detail(ci, ai, i)?;
                if d.fringes.contains(name) {
                    lines.push((ci, ai, i, d.id, accounts[ci][ai].rows[i].total));
                }
            }
            let input: Vec<(RowId, Cents)> = lines.iter().map(|l| (l.3, l.4)).collect();
            let (split, shares) = person_shares(ctx.arith, rate, cutoff, &input).ok_or_else(ovf)?;
            let mut codes: Vec<String> = Vec::new();
            for l in &lines {
                let c = code(l.0, l.1);
                if !codes.contains(&c) {
                    codes.push(c);
                }
            }
            let count = lines.len();
            let accounts = and_list(&codes);
            let who = match (codes.len() == 1, cutoff.is_some()) {
                (true, true) => say!(
                    "engine.working.person_paid_account",
                    person = person,
                    count = count,
                    accounts = accounts
                ),
                (false, true) => say!(
                    "engine.working.person_paid_accounts",
                    person = person,
                    count = count,
                    accounts = accounts
                ),
                (true, false) => say!(
                    "engine.working.person_paid_account_no_cutoff",
                    person = person,
                    count = count,
                    accounts = accounts,
                    fringe = name
                ),
                (false, false) => say!(
                    "engine.working.person_paid_accounts_no_cutoff",
                    person = person,
                    count = count,
                    accounts = accounts,
                    fringe = name
                ),
            };
            let rate_formula = if f.rate.0.trim().parse::<Dec>().is_ok() {
                None
            } else {
                Some(formula_words(ctx, &f.rate, rate))
            };
            for (l, share) in lines.iter().zip(&shares) {
                let (ci, ai, i, id, total) = *l;
                let mut notes = vec![
                    Step::note(say!("engine.working.label.person"), who.clone()),
                    Step::note_amount(
                        say!("engine.working.label.earnings"),
                        say!("engine.working.this_lines_total"),
                        total,
                    ),
                ];
                if let (Some(t), Some(c)) = (typed, cutoff) {
                    if ctx.b.cutoff_factor != CutoffFactor::One {
                        notes.push(cutoff_factor_note(ctx.b.cutoff_factor, t, c));
                    }
                }
                let pay: Cents = input
                    .iter()
                    .try_fold(0, |a: Cents, r| a.checked_add(r.1))
                    .ok_or_else(ovf)?;
                match split {
                    Split::Own => {
                        if let Some(c) = cutoff {
                            notes.push(Step::note_amount(
                                say!("engine.working.label.cutoff"),
                                say!(
                                    "engine.working.per_person_not_reached",
                                    cutoff = money(c),
                                    person = person,
                                    pay = money(pay),
                                    fringe = name
                                ),
                                pay,
                            ));
                        }
                        let exact = percent_of(ctx.arith, total, rate).ok_or_else(ovf)?;
                        notes.push(Step::note(
                            say!("engine.working.label.fringe"),
                            percent_words(rate, money(total), exact, rate_formula.clone()),
                        ));
                        notes.extend(rounding(exact, share.amount));
                    }
                    Split::Nothing => {
                        let c = cutoff.unwrap_or(0);
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_used_up"),
                            say!(
                                "engine.working.stops_per_person",
                                fringe = name,
                                cutoff = money(c)
                            ),
                            0,
                        ));
                    }
                    Split::Shared {
                        pay,
                        cutoff: c,
                        exact,
                        amount,
                    } => {
                        let text = match &rate_formula {
                            None => say!(
                                "engine.working.person_past_cutoff",
                                person = person,
                                pay = money(pay),
                                fringe = name,
                                cutoff = money(c),
                                percent = nwords(rate),
                                exact = dmoney(exact)
                            ),
                            Some(formula) => say!(
                                "engine.working.person_past_cutoff_rate",
                                person = person,
                                pay = money(pay),
                                fringe = name,
                                cutoff = money(c),
                                percent = nwords(rate),
                                exact = dmoney(exact),
                                formula = formula
                            ),
                        };
                        notes.push(Step::note_amount(
                            say!("engine.working.label.cutoff_reached"),
                            text,
                            amount,
                        ));
                        notes.extend(rounding(exact, amount));
                        let floor = if share.extra_cent {
                            share.amount.checked_sub(1).ok_or_else(ovf)?
                        } else {
                            share.amount
                        };
                        let code = if share.extra_cent {
                            "engine.working.share_extra_cent"
                        } else {
                            "engine.working.share"
                        };
                        let text = Words::said(
                            code,
                            vec![
                                ("person", person.to_string()),
                                ("amount", money(amount)),
                                ("total", money(total)),
                                ("pay", money(pay)),
                                ("floor", money(floor)),
                            ],
                        );
                        notes.push(Step::note(say!("engine.working.label.share"), text));
                    }
                }
                let kind_text = match cutoff {
                    Some(c) => say!(
                        "engine.working.percent_with_cutoff_per_person",
                        percent = nwords(rate),
                        cutoff = money(c)
                    ),
                    None => Words::from(format!("{}%", nwords(rate))),
                };
                let s = Step::value(name, kind_text, share.amount)
                    .with(notes)
                    .with_link(Link::Row { ci, ai, row: id })
                    .with_expand(Target::LineFringe(id, name.to_string()));
                found
                    .entry((ci, ai, i))
                    .or_default()
                    .insert(name.to_string(), s);
            }
        }
    }
    for ((ci, ai, i), steps) in found {
        let row = &mut accounts[ci][ai].rows[i];
        let mut map: BTreeMap<String, Step> =
            std::mem::take(&mut row.fringes).into_iter().collect();
        map.extend(steps);
        let mut total: Cents = 0;
        for s in map.values() {
            total = total.checked_add(s.amount.unwrap_or(0)).ok_or_else(ovf)?;
        }
        row.fringe_total = total;
        row.fringes = map.into_iter().collect();
    }
    for list in accounts.iter_mut() {
        for a in list.iter_mut() {
            a.fringe_total = a
                .rows
                .iter()
                .try_fold(0, |t: Cents, r| t.checked_add(r.fringe_total))
                .ok_or_else(ovf)?;
        }
    }
    Ok(())
}

/// The budget's cutoff factor applied to `typed`, giving `cutoff`.
fn cutoff_factor_note(factor: CutoffFactor, typed: Cents, cutoff: Cents) -> Step {
    Step::note(
        say!("engine.working.label.cutoff_factor"),
        say!(
            "engine.working.cutoff_factor",
            factor = factor.name(),
            typed = money(typed),
            cutoff = money(cutoff)
        ),
    )
}

/// "18.5% of 1,000.00 = 185.00", with the rate's formula after it when it
/// has one.
fn percent_words(
    rate: Num,
    base: String,
    exact: impl fmt::Display,
    formula: Option<String>,
) -> Words {
    match formula {
        None => say!(
            "engine.working.percent_of",
            percent = nwords(rate),
            base = base,
            exact = dmoney(exact)
        ),
        Some(formula) => say!(
            "engine.working.percent_of_rate",
            percent = nwords(rate),
            base = base,
            exact = dmoney(exact),
            formula = formula
        ),
    }
}

/// `1 hour`, `12 hours`.
fn hours_words(h: Dec) -> String {
    if h == Dec::ONE {
        "1 hour".into()
    } else {
        format!("{} hours", num(h))
    }
}

/// How a flat fringe's basis reads in the working.
fn basis_words(b: FlatBasis) -> &'static str {
    match b {
        FlatBasis::Amount => "amount only",
        FlatBasis::AmountCounts => "amount plus counts in X and 4X",
        FlatBasis::Quantity => "full quantity",
    }
}

/// The notes for flat fringe `name` on line `d` (its count `c`, `rate` per
/// `unit`, `exact` before rounding) and its value step's text: how many
/// units the line counts and why, then the fringe itself, e.g. "ZDAY
/// 100.00 per Day x 25 Days (amount only; 14 Hours in X ignored) =
/// 2,500.00".
fn flat_words(
    b: &Budget,
    name: &str,
    d: &Detail,
    unit: Option<&str>,
    c: &FlatCount,
    rate: Num,
    exact: Money,
) -> R<(Vec<Step>, Words)> {
    let fu = unit.map(str::trim).filter(|u| !u.is_empty());
    let fu_text = fu.unwrap_or("line");
    let count = c.count().ok_or_else(ovf)?;
    let lu = d.unit.trim();
    let qty = |v: Dec, u: &str| format!("{}{}", num(v), unit_words(b, u, v));
    let factor_words = |f: Factor, v: Dec| match f {
        Factor::X => format!("{} in X", qty(v, &d.unit2)),
        Factor::X4 => format!("{} in 4X", qty(v, &d.unit4)),
    };
    let old_quantity =
        c.basis == FlatBasis::Quantity && !matches!(c.from, CountFrom::Capped { .. });
    let mut paren: Vec<String> = Vec::new();
    let mut clauses: Vec<String> = Vec::new();
    // The count before X and 4X, and whether it is a "once".
    let (start, once) = match &c.from {
        CountFrom::PerLine => {
            paren.push("once per line".into());
            (Dec::ONE, true)
        }
        CountFrom::FringeNoHours(u) => {
            paren.push(format!("{} has no hours: charged once", u.trim()));
            (Dec::ONE, true)
        }
        CountFrom::LineNoHours(u) if u.trim().is_empty() => {
            paren.push("the line has no unit: charged once".into());
            (Dec::ONE, true)
        }
        CountFrom::LineNoHours(u) => {
            paren.push(format!("{} has no hours: charged once", u.trim()));
            (Dec::ONE, true)
        }
        CountFrom::NoAmount => {
            paren.push("the Amount is 0: nothing counted".into());
            (Dec::ZERO, true)
        }
        CountFrom::SameUnit => {
            paren.push(basis_words(c.basis).into());
            if old_quantity && (!d.x.is_blank() || !d.x4.is_blank()) {
                clauses.push(format!(
                    "the line's quantity (amount x X x 4X) is {}",
                    qty(c.base, lu)
                ));
            }
            (c.base, false)
        }
        CountFrom::Hours {
            line_hours,
            fringe_hours,
        } => {
            paren.push(basis_words(c.basis).into());
            let converted = Exact::dec(c.base)
                .checked_mul(*line_hours)
                .and_then(|e| e.div_to_dec(*fringe_hours))
                .ok_or_else(ovf)?;
            clauses.push(format!(
                "{}{} x {} per {lu} / {} per {fu_text} = {}",
                if old_quantity { "quantity " } else { "" },
                qty(c.base, lu),
                hours_words(*line_hours),
                hours_words(*fringe_hours),
                qty(converted, fu_text)
            ));
            (converted, false)
        }
        CountFrom::Capped {
            line_hours,
            fringe_hours,
            day_unit,
            day_hours,
            per_day,
            cap,
            from_x,
        } => {
            paren.push(basis_words(c.basis).into());
            let days = Exact::dec(c.base)
                .checked_mul(*line_hours)
                .and_then(|e| e.div_to_dec(*day_hours))
                .ok_or_else(ovf)?;
            let used = (*per_day).min(*cap);
            let days_text = if lu == day_unit {
                format!("{} days", num(days))
            } else {
                format!(
                    "{} x {} per {lu} / {} per {day_unit} = {} days",
                    qty(c.base, lu),
                    hours_words(*line_hours),
                    hours_words(*day_hours),
                    num(days)
                )
            };
            let per_day_text = if *from_x {
                format!("{} a day from X", hours_words(*per_day))
            } else {
                format!("{} a day (one {day_unit})", hours_words(*per_day))
            };
            let cap_text = if *per_day > *cap {
                paren.push(format!("capped at {} a day", hours_words(*cap)));
                format!("capped at {} a day", hours_words(*cap))
            } else {
                format!("within the cap of {} a day", hours_words(*cap))
            };
            let hours = days.checked_mul(used).ok_or_else(ovf)?;
            let converted = if *fringe_hours == Dec::ONE {
                hours
            } else {
                Exact::dec(hours)
                    .div_to_dec(*fringe_hours)
                    .ok_or_else(ovf)?
            };
            let per_unit = if *fringe_hours == Dec::ONE {
                String::new()
            } else {
                format!(" / {} per {fu_text}", hours_words(*fringe_hours))
            };
            clauses.push(format!(
                "{days_text}; {per_day_text}, {cap_text}: {} days x {}{per_unit} = {}",
                num(days),
                hours_words(used),
                qty(converted, fu_text)
            ));
            (converted, false)
        }
    };
    if !c.counted.is_empty() {
        if once {
            paren.push(basis_words(c.basis).into());
        }
        let parts: Vec<String> = c
            .counted
            .iter()
            .map(|&(f, v)| match f {
                Factor::X => format!("{} (X)", qty(v, &d.unit2)),
                Factor::X4 => format!("{} (4X)", qty(v, &d.unit4)),
            })
            .collect();
        let from = if once {
            "1 (once)".to_string()
        } else {
            qty(start, fu_text)
        };
        clauses.push(format!(
            "{from} x {} = {}",
            parts.join(" x "),
            if once {
                num(count)
            } else {
                qty(count, fu_text)
            }
        ));
    }
    if !c.ignored.is_empty() {
        let parts: Vec<String> = c.ignored.iter().map(|&(f, v)| factor_words(f, v)).collect();
        paren.push(format!("{} ignored", parts.join(" and ")));
    }
    let count_text = if once {
        num(count)
    } else {
        qty(count, fu_text)
    };
    let mut notes = Vec::new();
    if !clauses.is_empty() {
        notes.push(Step::note(
            say!("engine.working.label.count"),
            clauses.join("; "),
        ));
    }
    if let FringeKind::Flat {
        cutoff: Some(cut), ..
    } = &b.fringes[name].kind
    {
        notes.push(Step::note(
            say!("engine.working.label.cutoff"),
            say!(
                "engine.working.flat_cutoff_unused",
                cutoff = money(*cut),
                fringe = name
            ),
        ));
    }
    notes.push(Step::note(
        say!("engine.working.label.fringe"),
        format!(
            "{name} {} per {fu_text} x {count_text} ({}) = {}",
            dmoney(Money::num(rate)),
            paren.join("; "),
            dmoney(exact)
        ),
    ));
    let text = match (&c.from, fu, c.basis) {
        (CountFrom::PerLine, _, _) | (_, None, _) => say!("engine.working.flat_once_per_line"),
        (_, Some(u), FlatBasis::Amount) => say!("engine.working.flat_per_unit_amount", unit = u),
        (_, Some(u), FlatBasis::AmountCounts) => {
            say!("engine.working.flat_per_unit_counts", unit = u)
        }
        (_, Some(u), FlatBasis::Quantity) => {
            say!("engine.working.flat_per_unit_quantity", unit = u)
        }
    };
    Ok((notes, text))
}
