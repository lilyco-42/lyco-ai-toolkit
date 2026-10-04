//! Drive the Kaggle CLI: push a kernel, watch it, pull its output.
//!
//! Everything here shells out to `kaggle`, which the operator has already
//! authenticated. The interesting logic - deciding whether a run is finished,
//! and giving up instead of polling forever - is kept in pure functions so it
//! can be tested without a GPU or a network.

use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Where a kernel run has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Queued,
    Running,
    Complete,
    Error,
    Cancelled,
    /// A state this build does not know; kept so it can be reported verbatim.
    Unknown(String),
}

impl Status {
    pub fn name(&self) -> &str {
        match self {
            Status::Queued => "QUEUED",
            Status::Running => "RUNNING",
            Status::Complete => "COMPLETE",
            Status::Error => "ERROR",
            Status::Cancelled => "CANCELLED",
            Status::Unknown(raw) => raw,
        }
    }

    /// True when no further polling can change anything.
    ///
    /// `Unknown` counts as terminal on purpose. If Kaggle reports a state this
    /// build does not recognise, waiting will not teach us what it means, and
    /// polling until the budget runs out would hang the job instead of
    /// reporting the state by name so a human can act on it.
    pub fn is_terminal(&self) -> bool {
        !matches!(self, Status::Queued | Status::Running)
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Status::Complete)
    }
}

/// Parse the CLI's status line.
///
/// The shape is `<ref> has status "KernelWorkerStatus.COMPLETE"`. An
/// unrecognised state is preserved rather than mapped to a guess.
pub fn parse_status(output: &str) -> Option<Status> {
    let open = output.find('"')?;
    let rest = &output[open + 1..];
    let close = rest.find('"')?;
    let raw = rest[..close].trim();
    if raw.is_empty() {
        return None;
    }
    let name = raw.strip_prefix("KernelWorkerStatus.").unwrap_or(raw);
    Some(match name {
        "QUEUED" => Status::Queued,
        "RUNNING" => Status::Running,
        "COMPLETE" => Status::Complete,
        "ERROR" => Status::Error,
        "CANCELLED" | "CANCEL_ACKNOWLEDGED" | "CANCEL_REQUESTED" => Status::Cancelled,
        other => Status::Unknown(other.to_string()),
    })
}

/// Poll until the status is terminal, or give up.
///
/// The observer is a closure so the loop can be exercised with a scripted
/// sequence instead of a real GPU job.
pub fn wait_until_terminal<F>(
    mut observe: F,
    poll_every: Duration,
    max_polls: u32,
) -> Result<Status>
where
    F: FnMut() -> Result<Status>,
{
    if max_polls == 0 {
        bail!("max_polls must be at least 1");
    }
    let mut observed = 0u32;
    let mut last = Status::Unknown("never polled".to_string());
    while observed < max_polls {
        observed += 1;
        last = observe()?;
        if last.is_terminal() {
            return Ok(last);
        }
        if observed < max_polls {
            std::thread::sleep(poll_every);
        }
    }
    bail!(
        "gave up after {max_polls} polls, last status was {}",
        last.name()
    )
}

/// A thin wrapper over the `kaggle` executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kaggle {
    program: String,
}

impl Default for Kaggle {
    fn default() -> Self {
        Kaggle::new()
    }
}

impl Kaggle {
    pub fn new() -> Self {
        Kaggle {
            program: "kaggle".to_string(),
        }
    }

    pub fn with_program(program: impl Into<String>) -> Self {
        Kaggle {
            program: program.into(),
        }
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    /// True when the CLI answers at all.
    pub fn available(&self) -> bool {
        Command::new(&self.program)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn run(&self, args: &[String]) -> Result<String> {
        let output = Command::new(&self.program)
            .args(args)
            .output()
            .with_context(|| {
                format!(
                    "cannot run '{}': is the Kaggle CLI installed and on PATH?",
                    self.program
                )
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if !output.status.success() {
            // The CLI reports some failures on stdout, so show whichever spoke.
            let detail = match (stderr.trim().is_empty(), stdout.trim().is_empty()) {
                (false, _) => stderr.trim().to_string(),
                (true, false) => stdout.trim().to_string(),
                _ => "no output".to_string(),
            };
            bail!(
                "'{} {}' failed with {}: {detail}",
                self.program,
                args.join(" "),
                output.status
            );
        }
        Ok(stdout)
    }

    /// Upload a folder and start a run.
    ///
    /// Pushing an existing kernel creates a new version and runs it, which is
    /// how a job is actually started.
    pub fn push(&self, folder: &Path, timeout_secs: Option<u32>) -> Result<String> {
        if !folder.join("kernel-metadata.json").exists() {
            bail!(
                "{} has no kernel-metadata.json, so Kaggle cannot run it",
                folder.display()
            );
        }
        let mut args = vec![
            "kernels".to_string(),
            "push".to_string(),
            "-p".to_string(),
            folder.display().to_string(),
        ];
        if let Some(seconds) = timeout_secs {
            args.push("-t".to_string());
            args.push(seconds.to_string());
        }
        self.run(&args)
    }

    pub fn status(&self, kernel: &str) -> Result<Status> {
        let args = vec![
            "kernels".to_string(),
            "status".to_string(),
            kernel.to_string(),
        ];
        let text = self.run(&args)?;
        parse_status(&text)
            .with_context(|| format!("cannot read a status out of the CLI's output: {text:?}"))
    }

    /// Download a run's output files.
    pub fn download_output(&self, kernel: &str, into: &Path) -> Result<String> {
        std::fs::create_dir_all(into)
            .with_context(|| format!("cannot create {}", into.display()))?;
        let args = vec![
            "kernels".to_string(),
            "output".to_string(),
            kernel.to_string(),
            "-p".to_string(),
            into.display().to_string(),
            "-o".to_string(),
        ];
        self.run(&args)
    }

    /// Poll a running kernel until it settles.
    pub fn wait(&self, kernel: &str, poll_every: Duration, max_polls: u32) -> Result<Status> {
        wait_until_terminal(|| self.status(kernel), poll_every, max_polls)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn parses_the_status_line_the_cli_prints() {
        let line = "me/thing has status \"KernelWorkerStatus.COMPLETE\"";
        assert_eq!(parse_status(line), Some(Status::Complete));
        let queued = "me/thing has status \"KernelWorkerStatus.QUEUED\"";
        assert_eq!(parse_status(queued), Some(Status::Queued));
    }

    #[test]
    fn maps_every_state_we_have_seen() {
        let cases = [
            ("QUEUED", Status::Queued),
            ("RUNNING", Status::Running),
            ("COMPLETE", Status::Complete),
            ("ERROR", Status::Error),
            ("CANCEL_ACKNOWLEDGED", Status::Cancelled),
        ];
        for (name, want) in cases {
            let line = format!("k has status \"KernelWorkerStatus.{name}\"");
            assert_eq!(parse_status(&line), Some(want), "{name}");
        }
    }

    #[test]
    fn an_unknown_state_is_preserved_not_guessed() {
        let line = "k has status \"KernelWorkerStatus.SOMETHING_NEW\"";
        assert_eq!(
            parse_status(line),
            Some(Status::Unknown("SOMETHING_NEW".to_string()))
        );
    }

    #[test]
    fn tolerates_an_unprefixed_state() {
        assert_eq!(
            parse_status("k has status \"COMPLETE\""),
            Some(Status::Complete)
        );
    }

    #[test]
    fn refuses_to_invent_a_status_from_noise() {
        assert_eq!(parse_status(""), None);
        assert_eq!(parse_status("no quotes here"), None);
        assert_eq!(parse_status("has status \"\""), None);
        assert_eq!(parse_status("only one \" quote"), None);
    }

    #[test]
    fn terminal_and_success_are_different_questions() {
        assert!(!Status::Queued.is_terminal());
        assert!(!Status::Running.is_terminal());
        assert!(Status::Complete.is_terminal());
        assert!(Status::Complete.is_success());
        assert!(Status::Error.is_terminal());
        assert!(
            !Status::Error.is_success(),
            "finished is not the same as worked"
        );
        assert!(Status::Cancelled.is_terminal());
        // An unrecognised state stops the wait rather than polling for ever,
        // but it is not a success.
        let unknown = Status::Unknown(String::from("SOMETHING_NEW"));
        assert!(
            unknown.is_terminal(),
            "waiting will not decode an unknown state"
        );
        assert!(!unknown.is_success());
        assert_eq!(unknown.name(), "SOMETHING_NEW");
    }

    #[test]
    fn waiting_returns_as_soon_as_the_run_settles() {
        let script =
            RefCell::new(vec![Status::Queued, Status::Running, Status::Complete].into_iter());
        let calls = RefCell::new(0);
        let outcome = wait_until_terminal(
            || {
                *calls.borrow_mut() += 1;
                Ok(script.borrow_mut().next().unwrap_or(Status::Complete))
            },
            Duration::ZERO,
            10,
        )
        .expect("wait");
        assert_eq!(outcome, Status::Complete);
        assert_eq!(*calls.borrow(), 3, "must stop polling once terminal");
    }

    #[test]
    fn waiting_gives_up_instead_of_polling_forever() {
        let err = wait_until_terminal(|| Ok(Status::Running), Duration::ZERO, 3).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("gave up"), "{text}");
        assert!(text.contains("RUNNING"), "{text}");
    }

    #[test]
    fn zero_polls_is_a_programming_error_not_a_silent_success() {
        assert!(wait_until_terminal(|| Ok(Status::Complete), Duration::ZERO, 0).is_err());
    }

    #[test]
    fn an_observer_failure_stops_the_wait() {
        let outcome = wait_until_terminal(|| anyhow::bail!("the CLI went away"), Duration::ZERO, 5);
        let err = outcome.unwrap_err();
        assert!(err.to_string().contains("CLI went away"), "{err}");
    }

    #[test]
    fn pushing_a_folder_without_metadata_fails_before_touching_the_network() {
        let dir = std::env::temp_dir().join("lyco-kaggle-empty");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let err = Kaggle::new().push(&dir, None).unwrap_err();
        assert!(err.to_string().contains("kernel-metadata.json"), "{err}");
    }

    #[test]
    fn a_missing_cli_is_reported_clearly() {
        let kaggle = Kaggle::with_program("definitely-not-a-real-binary-xyz");
        assert!(!kaggle.available());
        let err = kaggle.status("me/thing").unwrap_err();
        assert!(err.to_string().contains("cannot run"), "{err}");
    }
}
