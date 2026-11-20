//! Overtime patterns and rate tables.

use super::*;

// ---------------------------------------------------------------------------
// Overtime patterns (method_core::overtime)

/// One overtime band: its multiplier, and how many hours it covers (left
/// out on the last band, which covers the rest).
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOtBand {
    multiplier: DecStr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hours: Option<DecStr>,
}

/// A saved overtime pattern as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOtPatternRaw {
    name: String,
    straight_hours: DecStr,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bands: Vec<FileOtBand>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sixth_day: Option<DecStr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    seventh_day: Option<DecStr>,
}

/// A saved overtime pattern, checked when read (the same rules as saving
/// one in the app).
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "FileOtPatternRaw", into = "FileOtPatternRaw")]
pub(crate) struct FileOtPattern(pub(crate) method_core::overtime::OtPattern);

impl From<FileOtPattern> for FileOtPatternRaw {
    fn from(p: FileOtPattern) -> FileOtPatternRaw {
        let method_core::overtime::OtPattern {
            name,
            straight_hours,
            bands,
            sixth_day,
            seventh_day,
        } = p.0;
        FileOtPatternRaw {
            name,
            straight_hours: DecStr(straight_hours),
            bands: bands
                .into_iter()
                .map(|b| FileOtBand {
                    multiplier: DecStr(b.multiplier),
                    hours: b.hours.map(DecStr),
                })
                .collect(),
            sixth_day: sixth_day.map(DecStr),
            seventh_day: seventh_day.map(DecStr),
        }
    }
}

impl TryFrom<FileOtPatternRaw> for FileOtPattern {
    type Error = String;

    fn try_from(f: FileOtPatternRaw) -> Result<FileOtPattern, String> {
        let p = method_core::overtime::OtPattern {
            name: f.name,
            straight_hours: f.straight_hours.0,
            bands: f
                .bands
                .into_iter()
                .map(|b| method_core::overtime::OtBand {
                    multiplier: b.multiplier.0,
                    hours: b.hours.map(|h| h.0),
                })
                .collect(),
            sixth_day: f.sixth_day.map(|d| d.0),
            seventh_day: f.seventh_day.map(|d| d.0),
        };
        p.validate_all()
            .map_err(|e| format!("overtime pattern {:?}: {e}", p.name))?;
        Ok(FileOtPattern(p))
    }
}

// ---------------------------------------------------------------------------
// Rate tables (method_core::rates; decisions.md, entry 90)

/// One row of a rate table as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRateRow {
    classification: String,
    rates: BTreeMap<String, DecStr>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
}

/// A rate table as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRateTableRaw {
    name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    notes: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rows: Vec<FileRateRow>,
}

/// A rate table, checked when read (the same rules as saving one in the
/// app).
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "FileRateTableRaw", into = "FileRateTableRaw")]
pub(crate) struct FileRateTable(pub(crate) method_core::rates::RateTable);

impl From<FileRateTable> for FileRateTableRaw {
    fn from(t: FileRateTable) -> FileRateTableRaw {
        let method_core::rates::RateTable { name, notes, rows } = t.0;
        FileRateTableRaw {
            name,
            notes,
            rows: rows
                .into_iter()
                .map(|r| {
                    let method_core::rates::RateRow {
                        classification,
                        rates,
                        from,
                        to,
                        notes,
                    } = r;
                    FileRateRow {
                        classification,
                        rates: rates.into_iter().map(|(u, v)| (u, DecStr(v))).collect(),
                        from,
                        to,
                        notes,
                    }
                })
                .collect(),
        }
    }
}

impl TryFrom<FileRateTableRaw> for FileRateTable {
    type Error = String;

    fn try_from(f: FileRateTableRaw) -> Result<FileRateTable, String> {
        let t = method_core::rates::RateTable {
            name: f.name,
            notes: f.notes,
            rows: f
                .rows
                .into_iter()
                .map(|r| method_core::rates::RateRow {
                    classification: r.classification,
                    rates: r.rates.into_iter().map(|(u, v)| (u, v.0)).collect(),
                    from: r.from,
                    to: r.to,
                    notes: r.notes,
                })
                .collect(),
        };
        t.validate()
            .map_err(|e| format!("rate table {:?}: {e}", t.name))?;
        Ok(FileRateTable(t))
    }
}

/// Where a line's rate came from, as written.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRateSourceRaw {
    table: String,
    classification: String,
    unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    as_of: Option<String>,
    rate: DecStr,
}

/// Where a line's rate came from, checked when read: names given, a real
/// date.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "FileRateSourceRaw", into = "FileRateSourceRaw")]
pub(crate) struct FileRateSource(pub(crate) method_core::rates::RateSource);

impl From<FileRateSource> for FileRateSourceRaw {
    fn from(s: FileRateSource) -> FileRateSourceRaw {
        let method_core::rates::RateSource {
            table,
            classification,
            unit,
            as_of,
            rate,
        } = s.0;
        FileRateSourceRaw {
            table,
            classification,
            unit,
            as_of,
            rate: DecStr(rate),
        }
    }
}

impl TryFrom<FileRateSourceRaw> for FileRateSource {
    type Error = String;

    fn try_from(f: FileRateSourceRaw) -> Result<FileRateSource, String> {
        if f.table.trim().is_empty()
            || f.classification.trim().is_empty()
            || f.unit.trim().is_empty()
        {
            return Err("a rate source needs its table, classification and unit".into());
        }
        if let Some(d) = &f.as_of {
            if !method_core::actuals::is_iso_date(d) {
                return Err(format!(
                    "a rate source's as_of {d:?} is not a date written YYYY-MM-DD"
                ));
            }
        }
        Ok(FileRateSource(method_core::rates::RateSource {
            table: f.table,
            classification: f.classification,
            unit: f.unit,
            as_of: f.as_of,
            rate: f.rate.0,
        }))
    }
}
