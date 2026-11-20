//! Explaining lines, accounts, categories and the topsheet totals.

use super::*;

impl<'a> Explainer<'a> {
    // -----------------------------------------------------------------------
    // Lines

    pub(super) fn line(&self, id: RowId) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let d = &acct.details[i];
        let r = &self.accounts[ci][ai].rows[i];
        let title = row_title(&acct.code, i, d, RowPart::Total);
        match d.kind {
            RowKind::Separator => Err(ExplainError(
                "a separator is a drawn rule: it has no total".into(),
            )),
            RowKind::Subtotal => {
                let calc = &self.accounts[ci][ai];
                let children = r
                    .adds
                    .iter()
                    .filter_map(|&k| calc.rows[k].value.clone())
                    .collect();
                let label = match d.description.trim() {
                    "" => say!("engine.working.label.subtotal"),
                    t => Words::from(t),
                };
                let s = Step::sum(label, say!("engine.working.subtotal_adds"), children)?
                    .with_link(Link::Row { ci, ai, row: id });
                Ok((title, s))
            }
            RowKind::Line | RowKind::Adjustment => {
                let mut s = r.value.clone().ok_or_else(|| gone("that line"))?;
                if r.included && r.fringe_total != 0 {
                    s.children.push(
                        Step::note_amount(
                            say!("engine.working.label.fringes"),
                            say!("engine.working.fringes_on_top"),
                            r.fringe_total,
                        )
                        .with_expand(Target::LineFringes(id)),
                    );
                }
                Ok((title, s))
            }
        }
    }

    /// The fringe steps of row `i` as a sum.
    fn row_fringes_step(&self, ci: usize, ai: usize, i: usize) -> R<Step> {
        let acct = self.acct(ci, ai);
        let d = &acct.details[i];
        let r = &self.accounts[ci][ai].rows[i];
        let mut children: Vec<Step> = r.fringes.iter().map(|(_, s)| s.clone()).collect();
        if !r.included && !d.fringes.is_empty() {
            children.push(Step::note(
                say!("engine.working.label.left_out"),
                say!("engine.working.left_out_no_fringes"),
            ));
        }
        Ok(Step::sum(
            line_label(i, d),
            say!("engine.working.fringes_by_line", line = i + 1),
            children,
        )?
        .with_link(Link::Row { ci, ai, row: d.id })
        .with_expand(Target::LineFringes(d.id)))
    }

    pub(super) fn line_fringes(&self, id: RowId) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let title = row_title(&acct.code, i, &acct.details[i], RowPart::Fringes);
        let mut s = self.row_fringes_step(ci, ai, i)?;
        s.expand = None;
        if s.children.is_empty() {
            s.children.push(Step::note(
                say!("engine.working.label.no_fringes"),
                say!("engine.working.earns_none"),
            ));
        }
        s.children.push(self.posting_note());
        Ok((title, s))
    }

    pub(super) fn line_fringe(&self, id: RowId, name: &str) -> R<(Words, Step)> {
        let (ci, ai, i) = self.find_row(id)?;
        let acct = self.acct(ci, ai);
        let r = &self.accounts[ci][ai].rows[i];
        let title = row_title(&acct.code, i, &acct.details[i], RowPart::Fringe(name));
        let s = r
            .fringes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| {
                Step::value(name, say!("engine.working.earns_nothing_from_it"), 0)
                    .with_link(Link::Row { ci, ai, row: id })
            });
        Ok((title, s))
    }

    // -----------------------------------------------------------------------
    // Accounts and categories

    fn acct(&self, ci: usize, ai: usize) -> &crate::model::Account {
        &self
            .b
            .categories()
            .nth(ci)
            .expect("checked by the caller")
            .accounts[ai]
    }

    fn check_account(&self, ci: usize, ai: usize) -> R<()> {
        self.accounts
            .get(ci)
            .and_then(|c| c.get(ai))
            .map(|_| ())
            .ok_or_else(|| gone("that account"))
    }

    pub(super) fn account_name(&self, ci: usize, ai: usize) -> String {
        let a = self.acct(ci, ai);
        format!("{} {}", a.code, a.name).trim().to_string()
    }

    /// The account's rows as a sum (subtotal and separator rows add nothing
    /// and are left out).
    fn lines_step(&self, ci: usize, ai: usize) -> R<Step> {
        let calc = &self.accounts[ci][ai];
        let mut children: Vec<Step> = calc.rows.iter().filter_map(|r| r.value.clone()).collect();
        if self.b.round_account_totals {
            let lines = calc
                .rows
                .iter()
                .try_fold(0, |a: Cents, r| a.checked_add(r.total))
                .ok_or_else(ovf)?;
            let whole =
                super::super::arith::whole_units(self.b.arithmetic, lines).ok_or_else(ovf)?;
            if whole != lines {
                let text = match self.b.arithmetic {
                    Arithmetic::Mmb => say!(
                        "engine.working.whole_units_half_up",
                        lines = money(lines),
                        whole = money(whole)
                    ),
                    Arithmetic::Exact => say!(
                        "engine.working.whole_units_half_away",
                        lines = money(lines),
                        whole = money(whole)
                    ),
                };
                children.push(Step::value(
                    say!("engine.working.label.rounding"),
                    text,
                    whole.checked_sub(lines).ok_or_else(ovf)?,
                ));
            }
        }
        Ok(Step::sum(
            say!("engine.working.label.lines"),
            say!(
                "engine.working.every_line_of",
                account = self.account_name(ci, ai)
            ),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::AccountLines { ci, ai }))
    }

    fn account_fringes_step(&self, ci: usize, ai: usize) -> R<Step> {
        let acct = self.acct(ci, ai);
        let mut children = Vec::new();
        for (i, d) in acct.details.iter().enumerate() {
            if d.kind.is_costed() && !self.accounts[ci][ai].rows[i].fringes.is_empty() {
                children.push(self.row_fringes_step(ci, ai, i)?);
            }
        }
        Ok(Step::sum(
            self.account_name(ci, ai),
            say!("engine.working.account_fringes"),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::AccountFringes { ci, ai }))
    }

    /// The account's total as its category counts it.
    fn account_step(&self, ci: usize, ai: usize) -> R<Step> {
        let mut children = vec![self.lines_step(ci, ai)?];
        if self.b.fringe_posting == FringePosting::Account {
            let mut f = self.account_fringes_step(ci, ai)?;
            f.relabel(say!("engine.working.label.fringes"));
            f.retext(say!("engine.working.posted_to_account"));
            children.push(f);
        } else if self.accounts[ci][ai].fringe_total != 0 {
            children.push(
                Step::note_amount(
                    say!("engine.working.label.fringes_earned_here"),
                    self.not_in_this_total(),
                    self.accounts[ci][ai].fringe_total,
                )
                .with_expand(Target::AccountFringes { ci, ai }),
            );
        }
        Ok(Step::sum(
            self.account_name(ci, ai),
            say!("engine.working.account_total"),
            children,
        )?
        .with_link(Link::Account { ci, ai })
        .with_expand(Target::Account { ci, ai }))
    }

    pub(super) fn account(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_step(ci, ai)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.account_total",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    pub(super) fn account_lines(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.lines_step(ci, ai)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.account_lines",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    pub(super) fn account_fringes(&self, ci: usize, ai: usize) -> R<(Words, Step)> {
        self.check_account(ci, ai)?;
        let mut s = self.account_fringes_step(ci, ai)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = say!(
            "engine.working.title.account_fringes",
            account = self.account_name(ci, ai)
        );
        Ok((title, s))
    }

    pub(super) fn category_name(&self, ci: usize) -> String {
        let c = self.b.categories().nth(ci).expect("checked by the caller");
        format!("{} {}", c.code, c.name).trim().to_string()
    }

    fn check_category(&self, ci: usize) -> R<()> {
        if ci < self.accounts.len() {
            Ok(())
        } else {
            Err(gone("that category"))
        }
    }

    fn category_fringes_step(&self, ci: usize) -> R<Step> {
        let children = (0..self.accounts[ci].len())
            .filter(|&ai| self.accounts[ci][ai].fringe_total != 0 || self.has_fringes(ci, ai))
            .map(|ai| self.account_fringes_step(ci, ai))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum(
            self.category_name(ci),
            say!("engine.working.category_fringes"),
            children,
        )?
        .with_link(Link::Category { ci })
        .with_expand(Target::CategoryFringes(ci)))
    }

    fn has_fringes(&self, ci: usize, ai: usize) -> bool {
        self.accounts[ci][ai]
            .rows
            .iter()
            .any(|r| !r.fringes.is_empty())
    }

    fn category_step(&self, ci: usize) -> R<Step> {
        let mut children = (0..self.accounts[ci].len())
            .map(|ai| self.account_step(ci, ai))
            .collect::<R<Vec<_>>>()?;
        match &self.b.fringe_posting {
            FringePosting::Category { .. } => {
                let number = self
                    .report
                    .categories
                    .get(ci)
                    .and_then(|c| c.fringe_account.as_ref())
                    .map(|(n, _)| n.clone())
                    .unwrap_or_default();
                let mut f = self.category_fringes_step(ci)?;
                f.relabel(match number.trim() {
                    "" => say!("engine.working.label.fringes"),
                    _ => say!(
                        "engine.working.label.numbered_fringes",
                        number = number.trim_start()
                    ),
                });
                f.retext(say!("engine.working.category_fringe_account"));
                children.push(f);
            }
            FringePosting::Account => {}
            _ => {
                let total = self.category_fringe_total(ci)?;
                if total != 0 {
                    children.push(
                        Step::note_amount(
                            say!("engine.working.label.fringes_earned_here"),
                            self.not_in_this_total(),
                            total,
                        )
                        .with_expand(Target::CategoryFringes(ci)),
                    );
                }
            }
        }
        Ok(Step::sum(
            self.category_name(ci),
            say!("engine.working.category_total"),
            children,
        )?
        .with_link(Link::Category { ci })
        .with_expand(Target::Category(ci)))
    }

    fn category_fringe_total(&self, ci: usize) -> R<Cents> {
        self.accounts[ci]
            .iter()
            .try_fold(0, |a: Cents, c| a.checked_add(c.fringe_total))
            .ok_or_else(ovf)
    }

    pub(super) fn category(&self, ci: usize) -> R<(Words, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_step(ci)?;
        s.expand = None;
        let title = say!(
            "engine.working.title.category_total",
            category = self.category_name(ci)
        );
        Ok((title, s))
    }

    pub(super) fn category_fringes(&self, ci: usize) -> R<(Words, Step)> {
        self.check_category(ci)?;
        let mut s = self.category_fringes_step(ci)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = say!(
            "engine.working.title.category_fringes",
            category = self.category_name(ci)
        );
        Ok((title, s))
    }

    // -----------------------------------------------------------------------
    // Topsheet

    /// "not in this total: " and where fringes post.
    fn not_in_this_total(&self) -> Words {
        match &self.b.fringe_posting {
            FringePosting::Budget => say!("engine.working.not_in_total_budget"),
            FringePosting::ProductionLevel => say!("engine.working.not_in_total_level"),
            FringePosting::Category { .. } => say!("engine.working.not_in_total_category"),
            FringePosting::Account => say!("engine.working.not_in_total_account"),
        }
    }

    fn posting_note(&self) -> Step {
        let text = match &self.b.fringe_posting {
            FringePosting::Budget => say!("engine.working.posting_budget"),
            FringePosting::ProductionLevel => say!("engine.working.posting_level"),
            FringePosting::Category { .. } => say!("engine.working.posting_category"),
            FringePosting::Account => say!("engine.working.posting_account"),
        };
        Step::note(say!("engine.working.label.where_fringes_post"), text)
    }

    fn level_fringes_step(&self, li: usize) -> R<Step> {
        let lv = self
            .report
            .levels
            .get(li)
            .ok_or_else(|| gone("that production level"))?;
        let dropped = dropped_from(self.b).is_some_and(|d| lv.start >= d);
        let children = if dropped {
            vec![Step::note(
                say!("engine.working.label.left_out"),
                say!("engine.working.trailing_fringes_dropped"),
            )]
        } else if self.b.fringe_posting == FringePosting::ProductionLevel {
            (lv.start..lv.end)
                .map(|ci| self.category_fringes_step(ci))
                .collect::<R<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Step::sum(
            say!("engine.working.label.fringes"),
            say!("engine.working.posted_above_level"),
            children,
        )?
        .with_expand(Target::LevelFringes(li)))
    }

    /// The production total's name, or `None` for the categories after the
    /// last one.
    fn level_name(&self, li: usize) -> Option<String> {
        match self.report.levels.get(li) {
            Some(l) if !l.name.is_empty() => Some(l.name.clone()),
            _ => None,
        }
    }

    fn level_step(&self, li: usize) -> R<Step> {
        let lv = self
            .report
            .levels
            .get(li)
            .ok_or_else(|| gone("that production level"))?;
        let mut children = (lv.start..lv.end)
            .map(|ci| self.category_step(ci))
            .collect::<R<Vec<_>>>()?;
        if self.b.fringe_posting == FringePosting::ProductionLevel {
            children.push(self.level_fringes_step(li)?);
        }
        let label = match self.level_name(li) {
            Some(n) => Words::from(n),
            None => say!("engine.working.label.after_last_level"),
        };
        Ok(
            Step::sum(label, say!("engine.working.level_adds"), children)?
                .with_expand(Target::Level(li)),
        )
    }

    pub(super) fn level(&self, li: usize) -> R<(Words, Step)> {
        let mut s = self.level_step(li)?;
        s.expand = None;
        let title = match self.level_name(li) {
            Some(n) => say!("engine.working.title.level_total", level = n),
            None => say!("engine.working.title.after_last_level_total"),
        };
        Ok((title, s))
    }

    pub(super) fn level_fringes(&self, li: usize) -> R<(Words, Step)> {
        let mut s = self.level_fringes_step(li)?;
        s.expand = None;
        s.children.push(self.posting_note());
        let title = match self.level_name(li) {
            Some(n) => say!("engine.working.title.level_fringes", level = n),
            None => say!("engine.working.title.after_last_level_fringes"),
        };
        Ok((title, s))
    }

    /// How many levels a real production total closes (the first is
    /// above the line only when one does).
    fn closed_levels(&self) -> usize {
        self.b
            .topsheet
            .iter()
            .filter(|r| matches!(r, TopsheetRow::ProductionTotal(_)))
            .count()
    }

    /// The levels a production total closes, as indexes into the report's
    /// levels (the trailing unnamed level is left out).
    fn closed_range(&self) -> std::ops::Range<usize> {
        0..self.closed_levels().min(self.report.levels.len())
    }

    fn atl_step(&self) -> R<Step> {
        let closed = self.closed_range().end;
        let split = usize::from(closed > 0);
        let mut children = (0..split)
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        if split == 0 {
            children.push(Step::note(
                say!("engine.working.label.no_production_totals"),
                say!("engine.working.no_production_totals"),
            ));
        }
        Ok(Step::sum(
            say!("engine.working.label.atl"),
            say!("engine.working.atl_is"),
            children,
        )?
        .with_expand(Target::Atl))
    }

    fn btl_step(&self) -> R<Step> {
        let closed = self.closed_range().end;
        let split = usize::from(closed > 0);
        let mut children = (split..closed)
            .map(|li| self.level_step(li))
            .collect::<R<Vec<_>>>()?;
        if closed < self.report.levels.len() {
            children.push(Step::note(
                say!("engine.working.label.not_counted_here"),
                say!("engine.working.after_last_level_neither"),
            ));
        }
        Ok(Step::sum(
            say!("engine.working.label.btl"),
            say!("engine.working.btl_is"),
            children,
        )?
        .with_expand(Target::Btl))
    }

    /// The trailing unnamed level, when categories follow the last
    /// production total (every category when there is none).
    fn trailing_steps(&self) -> R<Vec<Step>> {
        (self.closed_range().end..self.report.levels.len())
            .map(|li| self.level_step(li))
            .collect()
    }

    fn atl_btl_step(&self) -> R<Step> {
        Ok(Step::sum(
            say!("engine.working.label.atl_btl"),
            say!("engine.working.atl_btl_is"),
            vec![self.atl_step()?, self.btl_step()?],
        )?
        .with_expand(Target::AtlBtl))
    }

    pub(super) fn atl(&self) -> R<(Words, Step)> {
        let mut s = self.atl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.atl"), s))
    }

    pub(super) fn btl(&self) -> R<(Words, Step)> {
        let mut s = self.btl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.btl"), s))
    }

    pub(super) fn atl_btl(&self) -> R<(Words, Step)> {
        let mut s = self.atl_btl_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.atl_btl"), s))
    }

    fn budget_fringes_step(&self) -> R<Step> {
        let children = if self.b.fringe_posting == FringePosting::Budget {
            (0..self.accounts.len())
                .map(|ci| self.category_fringes_step(ci))
                .collect::<R<Vec<_>>>()?
        } else {
            Vec::new()
        };
        Ok(Step::sum(
            say!("engine.working.label.total_fringes"),
            say!("engine.working.total_fringes_is"),
            children,
        )?
        .with_expand(Target::BudgetFringes))
    }

    pub(super) fn budget_fringes(&self) -> R<(Words, Step)> {
        let mut s = self.budget_fringes_step()?;
        s.expand = None;
        s.children.push(self.posting_note());
        Ok((say!("engine.working.label.total_fringes"), s))
    }

    /// Everything before charges: above and below-the-line, the categories
    /// after the last production total, and the budget's fringe row.
    pub(super) fn before_charges_steps(&self) -> R<Vec<Step>> {
        let mut children = vec![self.atl_btl_step()?];
        children.extend(self.trailing_steps()?);
        if self.b.fringe_posting == FringePosting::Budget {
            children.push(self.budget_fringes_step()?);
        }
        Ok(children)
    }

    pub(super) fn grand_total(&self) -> R<(Words, Step)> {
        let mut children = self.before_charges_steps()?;
        for i in 0..self.report.charges.len() {
            children.push(self.charge_step(i)?);
        }
        let s = Step::sum(
            say!("engine.working.label.grand_total"),
            say!("engine.working.grand_total_is"),
            children,
        )?;
        Ok((say!("engine.working.label.grand_total"), s))
    }

    fn credits_total_step(&self) -> R<Step> {
        let children = (0..self.report.credits.len())
            .map(|i| self.credit_step(i))
            .collect::<R<Vec<_>>>()?;
        Ok(Step::sum(
            say!("engine.working.label.credits"),
            say!("engine.working.every_applied_credit"),
            children,
        )?
        .with_expand(Target::CreditsTotal))
    }

    pub(super) fn credits_total(&self) -> R<(Words, Step)> {
        let mut s = self.credits_total_step()?;
        s.expand = None;
        Ok((say!("engine.working.label.credits"), s))
    }

    pub(super) fn net_total(&self) -> R<(Words, Step)> {
        let (_, mut grand) = self.grand_total()?;
        grand.expand = Some(Target::GrandTotal);
        let mut credits = self.credits_total_step()?;
        credits.minus = true;
        credits.relabel(say!("engine.working.label.less_credits"));
        let s = Step::sum(
            say!("engine.working.label.net_total"),
            say!("engine.working.net_total_is"),
            vec![grand, credits],
        )?;
        Ok((say!("engine.working.label.net_total"), s))
    }

    pub(super) fn fringe(&self, name: &str) -> R<(Words, Step)> {
        let f = self
            .b
            .fringes
            .get(name)
            .ok_or_else(|| gone(&format!("fringe {name}")))?;
        let mut accounts = Vec::new();
        for (ci, cat) in self.b.categories().enumerate() {
            for (ai, _) in cat.accounts.iter().enumerate() {
                let lines: Vec<Step> = self.accounts[ci][ai]
                    .rows
                    .iter()
                    .flat_map(|r| r.fringes.iter())
                    .filter(|(n, _)| n == name)
                    .map(|(_, s)| s.clone())
                    .collect();
                if !lines.is_empty() {
                    accounts.push(
                        Step::sum(
                            self.account_name(ci, ai),
                            say!("engine.working.fringe_on_account_lines", fringe = name),
                            lines,
                        )?
                        .with_link(Link::Account { ci, ai }),
                    );
                }
            }
        }
        let kind = match &f.kind {
            FringeKind::Percent { cutoff: Some(c) } => {
                let factor = self.b.cutoff_factor;
                match factored_cutoff(self.b.arithmetic, Some(*c), factor) {
                    Some(Some(eff)) if factor != CutoffFactor::One => say!(
                        "engine.working.fringe_kind_percent_factor",
                        cutoff = money(eff),
                        typed = money(*c),
                        factor = factor.name()
                    ),
                    _ => say!(
                        "engine.working.fringe_kind_percent_cutoff",
                        cutoff = money(*c)
                    ),
                }
            }
            FringeKind::Percent { cutoff: None } => say!("engine.working.fringe_kind_percent"),
            FringeKind::Flat { .. } => say!("engine.working.fringe_kind_flat"),
        };
        let s = Step::sum(name, kind, accounts)?;
        Ok((say!("engine.working.title.fringe_total", fringe = name), s))
    }
}

/// "Key grip" or "Line 3" when it has no description.
pub(super) fn line_label(i: usize, d: &Detail) -> Words {
    let t = d.description.trim();
    if t.is_empty() {
        say!("engine.working.label.line_n", line = i + 1)
    } else {
        Words::from(t)
    }
}

/// What of a row an explanation's title names.
enum RowPart<'a> {
    Total,
    Fringes,
    Fringe(&'a str),
}

/// "2100 line 3 (Key grip): total", "2100 line 3: FICA".
fn row_title(code: &str, i: usize, d: &Detail, part: RowPart<'_>) -> Words {
    let t = d.description.trim();
    let line = i + 1;
    match (t.is_empty(), part) {
        (true, RowPart::Total) => {
            say!("engine.working.title.line_total", code = code, line = line)
        }
        (false, RowPart::Total) => say!(
            "engine.working.title.line_described_total",
            code = code,
            line = line,
            description = t
        ),
        (true, RowPart::Fringes) => {
            say!(
                "engine.working.title.line_fringes",
                code = code,
                line = line
            )
        }
        (false, RowPart::Fringes) => say!(
            "engine.working.title.line_described_fringes",
            code = code,
            line = line,
            description = t
        ),
        (true, RowPart::Fringe(f)) => say!(
            "engine.working.title.line_fringe",
            code = code,
            line = line,
            fringe = f
        ),
        (false, RowPart::Fringe(f)) => say!(
            "engine.working.title.line_described_fringe",
            code = code,
            line = line,
            description = t,
            fringe = f
        ),
    }
}
