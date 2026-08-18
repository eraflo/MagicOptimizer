//! Fetching the data artifacts the app ships without.
//!
//! # Why there is no server
//!
//! The artifacts are published as **GitHub release assets**, which are static files on a CDN.
//! There is nothing to host and nothing to run: the app fetches a URL, exactly as it already
//! fetches card images from Scryfall's CDN. That keeps invariant 7 intact — no account, no
//! telemetry, nothing about the user leaving the device — because a download says nothing except
//! that somebody wanted a file everyone can have.
//!
//! # Why this is in Rust and not in the WebView
//!
//! The content security policy in `tauri.conf.json` allows the page to reach exactly one host,
//! for images. Widening it so the frontend could download 26 MB of catalog would undo a
//! deliberate restriction for no gain. Rust has no such limit, and a command is the natural
//! place for something that writes to the app's data directory anyway.
//!
//! `reqwest` is already in the tree — Tauri depends on it and has already chosen a TLS backend.
//! No provider is named in `Cargo.toml` on purpose: asking for `rustls` explicitly pulls in
//! `aws-lc-rs`, which is C, and Cargo's feature unification means the app links whatever Tauri
//! enabled either way. Invariant 1 is about the crates under `crates/`; `src-tauri` was always
//! going to link whatever the shell needs.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

use crate::state::AppState;

type CommandResult<T> = Result<T, String>;

/// Where the artifacts live.
///
/// The rolling `nightly` release, because that is what the build publishes. A tagged release
/// would be steadier, and there is not one yet.
const BASE: &str = "https://github.com/eraflo/MagicOptimizer/releases/download/nightly";

/// One downloadable artifact.
pub struct Artifact {
    /// Key the UI passes back.
    pub name: &'static str,
    pub file: &'static str,
    pub label: &'static str,
    pub about: &'static str,
    /// Rough download size, so the UI can warn before starting one.
    pub megabytes: u32,
    /// Whether the app is usable without it.
    pub required: bool,
}

/// Everything the app can fetch, in the order it should be offered.
///
/// The catalog first because nothing works without it, then the two that each unlock one
/// feature. Sizes are measured, not estimated — see `docs/dev/data-pipeline.md`.
pub const ARTIFACTS: &[Artifact] = &[
    Artifact {
        name: "cards",
        file: "cards.rkyv",
        label: "Card data",
        about: "Every card, its rules, legalities and roles. Nothing works without it.",
        megabytes: 26,
        required: true,
    },
    Artifact {
        name: "artwork",
        file: "arthashes.bin",
        label: "Artwork fingerprints",
        about: "Lets the camera name the cards it sees. No images, only fingerprints.",
        megabytes: 6,
        required: false,
    },
    Artifact {
        name: "combos",
        file: "combos.rkyv",
        label: "Combo database",
        about: "Two-card combos and Commander bracket checks.",
        megabytes: 54,
        required: false,
    },
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStatus {
    pub name: String,
    pub label: String,
    pub about: String,
    pub megabytes: u32,
    pub required: bool,
    pub installed: bool,
    /// Size on disk, so a half-finished file is visible rather than mistaken for a whole one.
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    name: String,
    received: u64,
    /// Zero when the server does not say, which the UI has to handle rather than divide by.
    total: u64,
}

fn find(name: &str) -> CommandResult<&'static Artifact> {
    ARTIFACTS
        .iter()
        .find(|artifact| artifact.name == name)
        .ok_or_else(|| format!("{name:?} is not one of the downloadable artifacts"))
}

fn data_dir(app: &AppHandle) -> CommandResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("could not find the app's data directory: {e}"))
}

#[tauri::command]
pub fn artifacts_status(app: AppHandle) -> CommandResult<Vec<ArtifactStatus>> {
    let dir = data_dir(&app)?;
    Ok(ARTIFACTS
        .iter()
        .map(|artifact| {
            let path = dir.join(artifact.file);
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            ArtifactStatus {
                name: artifact.name.to_owned(),
                label: artifact.label.to_owned(),
                about: artifact.about.to_owned(),
                megabytes: artifact.megabytes,
                required: artifact.required,
                installed: bytes > 0,
                bytes,
            }
        })
        .collect())
}

/// Downloads one artifact into the app's data directory.
///
/// Streams to a `.partial` file and renames on success, so an interrupted download never leaves
/// a truncated file that the next run would happily treat as installed — the same rule the
/// artifact build follows for its own cache.
///
/// Progress is emitted as `artifact-progress` rather than returned, because a 54 MB download
/// that reports nothing for a minute is indistinguishable from a hang.
#[tauri::command]
pub async fn artifacts_download(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> CommandResult<u64> {
    let artifact = find(&name)?;
    let dir = data_dir(&app)?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    let url = format!("{BASE}/{}", artifact.file);
    let response = reqwest::get(&url)
        .await
        .map_err(|e| format!("could not reach {url}: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "{} is not published yet ({}). The build produces it; someone has to attach it to \
             the release.",
            artifact.file,
            response.status()
        ));
    }

    let total = response.content_length().unwrap_or(0);
    let partial = dir.join(format!("{}.partial", artifact.file));
    let mut file = tokio::fs::File::create(&partial)
        .await
        .map_err(|e| format!("could not write {}: {e}", partial.display()))?;

    let mut received = 0u64;
    let mut stream = response;
    // Emitted at most every quarter megabyte: a progress event per chunk would flood the bridge
    // with more messages than the download has packets.
    let mut last_reported = 0u64;

    while let Some(chunk) = stream
        .chunk()
        .await
        .map_err(|e| format!("the download was interrupted: {e}"))?
    {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("could not write {}: {e}", partial.display()))?;
        received += chunk.len() as u64;

        if received - last_reported > 256 * 1024 {
            last_reported = received;
            let _ = app.emit(
                "artifact-progress",
                Progress {
                    name: artifact.name.to_owned(),
                    received,
                    total,
                },
            );
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("could not finish writing {}: {e}", partial.display()))?;
    drop(file);

    let destination = dir.join(artifact.file);
    std::fs::rename(&partial, &destination)
        .map_err(|e| format!("could not move {} into place: {e}", partial.display()))?;

    // Loading it is the check. There are no published checksums to compare against, and every
    // reader here already validates its own magic number and format version — so asking the app
    // to open the file proves rather more than a hash would have.
    match artifact.name {
        "cards" => state.reload_catalog(),
        "artwork" => state.reload_artwork(),
        "combos" => state.reload_combos(),
        _ => {}
    }

    let _ = app.emit(
        "artifact-progress",
        Progress {
            name: artifact.name.to_owned(),
            received,
            total: received,
        },
    );
    Ok(received)
}

/// Deletes an artifact, for someone who wants the space back.
#[tauri::command]
pub fn artifacts_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> CommandResult<()> {
    let artifact = find(&name)?;
    let path = data_dir(&app)?.join(artifact.file);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("could not delete {}: {e}", path.display()))?;
    }
    match artifact.name {
        "cards" => state.reload_catalog(),
        "artwork" => state.reload_artwork(),
        "combos" => state.reload_combos(),
        _ => {}
    }
    Ok(())
}

/// Where the files go, so the UI can say it.
#[tauri::command]
pub fn artifacts_directory(app: AppHandle) -> CommandResult<String> {
    Ok(data_dir(&app)?.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_artifact_has_a_distinct_name_and_file() {
        // The name is what the UI passes back and the file is what lands on disk; a collision
        // in either would download one artifact over another.
        let mut names: Vec<&str> = ARTIFACTS.iter().map(|a| a.name).collect();
        let mut files: Vec<&str> = ARTIFACTS.iter().map(|a| a.file).collect();
        names.sort_unstable();
        files.sort_unstable();
        let (n, f) = (names.len(), files.len());
        names.dedup();
        files.dedup();
        assert_eq!(names.len(), n);
        assert_eq!(files.len(), f);
    }

    #[test]
    fn an_unknown_artifact_is_refused_rather_than_fetched() {
        // The name reaches a URL. Accepting anything here would let the frontend ask for a file
        // that is not ours to write.
        assert!(find("cards").is_ok());
        assert!(find("../../etc/passwd").is_err());
        assert!(find("").is_err());
    }

    #[test]
    fn the_file_names_match_what_the_app_looks_for() {
        // `AppState` locates these by name. A rename on one side alone downloads a file the app
        // then never finds, and the screen keeps saying there is no data.
        let files: Vec<&str> = ARTIFACTS.iter().map(|a| a.file).collect();
        assert!(files.contains(&"cards.rkyv"));
        assert!(files.contains(&"arthashes.bin"));
        assert!(files.contains(&"combos.rkyv"));
    }

    #[test]
    fn only_the_catalog_is_required() {
        // The other two each unlock one feature and everything degrades to saying what it could
        // not check, which is the rule the whole app follows.
        let required: Vec<&str> = ARTIFACTS
            .iter()
            .filter(|a| a.required)
            .map(|a| a.name)
            .collect();
        assert_eq!(required, ["cards"]);
    }
}
