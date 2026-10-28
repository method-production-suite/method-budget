//! Named versions saved inside a budget: save, restore, rename and delete.
//!
//! A [`Version`] is a full copy of the budget's content (a [`BudgetSnapshot`]:
//! everything except the version list, so versions never nest). Versions are
//! kept oldest first. Ids are unique within the list: a new version gets one
//! more than the highest id in use.
//!
//! Restoring is reversible: the content being replaced is first saved as an
//! automatic version, so restoring that one puts everything back.
//!
//! The engine never reads the clock. Callers pass `created` (an ISO 8601
//! date-time) and it is stored as given. Names and notes are stored as given
//! too; length or blank-name rules belong to the application's commands, like
//! other display data.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use crate::model::{Budget, Version, VersionId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionError {
    /// No version has this id.
    NotFound(VersionId),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VersionError::NotFound(id) => write!(f, "there is no version {id}"),
        }
    }
}

impl std::error::Error for VersionError {}

/// One more than the highest id in use, or (only if that would overflow) the
/// smallest unused id from 1.
fn next_id(b: &Budget) -> VersionId {
    let max = b.versions.iter().map(|v| v.id).max().unwrap_or(0);
    max.checked_add(1).unwrap_or_else(|| {
        let used: BTreeSet<VersionId> = b.versions.iter().map(|v| v.id).collect();
        (1..=VersionId::MAX)
            .find(|i| !used.contains(i))
            .unwrap_or(VersionId::MAX)
    })
}

/// Saves the current content as a new version at the end of the list and
/// returns its id.
pub fn save_version(
    b: &mut Budget,
    name: impl Into<String>,
    created: impl Into<String>,
    note: Option<String>,
) -> VersionId {
    let id = next_id(b);
    let snapshot = b.snapshot();
    b.versions.push(Version {
        id,
        name: name.into(),
        created: created.into(),
        note,
        budget: Arc::new(snapshot),
    });
    id
}

/// Saves `content` (another budget's content, such as a shared copy that
/// came back) as a new version at the end of the list and returns its id.
/// The current content doesn't change.
pub fn add_version(
    b: &mut Budget,
    name: impl Into<String>,
    created: impl Into<String>,
    note: Option<String>,
    content: crate::model::BudgetSnapshot,
) -> VersionId {
    let id = next_id(b);
    b.versions.push(Version {
        id,
        name: name.into(),
        created: created.into(),
        note,
        budget: Arc::new(content),
    });
    id
}

/// The name given to the automatic version saved before a restore. Kept in
/// English in the file; method-session's catalog has the same words
/// (`engine.name.before_restoring`, tested there) so the app can show it
/// translated.
pub fn before_restore_name(restored: &str) -> String {
    format!("Before restoring \"{restored}\"")
}

/// Replaces the current content with version `id`'s content. The version list
/// is kept, and the content being replaced is first saved as a new version
/// (named by [`before_restore_name`], with `created` as its time), so the
/// restore can be undone by restoring that version. Returns the automatic
/// version's id. Nothing changes if `id` does not exist.
pub fn restore_version(
    b: &mut Budget,
    id: VersionId,
    created: impl Into<String>,
) -> Result<VersionId, VersionError> {
    let target = b.version(id).ok_or(VersionError::NotFound(id))?;
    let name = before_restore_name(&target.name);
    let content = (*target.budget).clone();
    let auto = save_version(b, name, created, None);
    let versions = std::mem::take(&mut b.versions);
    // Report, tag and line presets and the display settings are tools, not
    // content: the current ones stay.
    let presets = std::mem::take(&mut b.report_presets);
    let tags = std::mem::take(&mut b.tag_presets);
    let blocks = std::mem::take(&mut b.line_presets);
    let ot = std::mem::take(&mut b.ot_patterns);
    let rates = std::mem::take(&mut b.rate_tables);
    let display = std::mem::take(&mut b.display);
    let order = std::mem::take(&mut b.setup_order);
    let folders = std::mem::take(&mut b.setup_folders);
    let sources = std::mem::take(&mut b.setup_sources);
    // Where the budget came from is about the file, not its content.
    let provenance = b.provenance.take();
    // How versions are stored is a file setting.
    let compact = b.compact_versions;
    // Actuals are the production's spending, not budget content.
    let actuals = std::mem::take(&mut b.actuals);
    *b = content.into_budget(versions);
    b.actuals = actuals;
    b.setup_order = order;
    b.setup_folders = folders;
    b.setup_sources = sources;
    b.provenance = provenance;
    b.compact_versions = compact;
    b.report_presets = presets;
    b.tag_presets = tags;
    b.line_presets = blocks;
    b.ot_patterns = ot;
    b.rate_tables = rates;
    b.display = display;
    Ok(auto)
}

/// Removes version `id` and returns it.
pub fn delete_version(b: &mut Budget, id: VersionId) -> Result<Version, VersionError> {
    let i = b
        .versions
        .iter()
        .position(|v| v.id == id)
        .ok_or(VersionError::NotFound(id))?;
    Ok(b.versions.remove(i))
}

/// Renames version `id`.
pub fn rename_version(
    b: &mut Budget,
    id: VersionId,
    name: impl Into<String>,
) -> Result<(), VersionError> {
    let v = b
        .versions
        .iter_mut()
        .find(|v| v.id == id)
        .ok_or(VersionError::NotFound(id))?;
    v.name = name.into();
    Ok(())
}
