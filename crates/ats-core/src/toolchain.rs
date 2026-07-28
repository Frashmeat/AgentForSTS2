use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GodotInstallation {
    pub executable: PathBuf,
    pub version: String,
}

#[derive(Debug, Error)]
pub enum GodotValidationError {
    #[error("Godot executable is not a file: {0}")]
    NotAFile(PathBuf),
    #[error("failed to start Godot version check for {path}: {source}")]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to read Godot version output for {path}: {source}")]
    Output {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Godot version check timed out after {timeout_ms} ms: {path}")]
    Timeout { path: PathBuf, timeout_ms: u128 },
    #[error("Godot version check failed for {path} (exit {exit_code:?}): {output}")]
    VersionCommandFailed {
        path: PathBuf,
        exit_code: Option<i32>,
        output: String,
    },
    #[error("Godot 4.5.1 is required, but {path} reported: {version}")]
    UnsupportedVersion { path: PathBuf, version: String },
}

pub fn validate_godot_executable(
    path: &Path,
    timeout: Duration,
) -> Result<GodotInstallation, GodotValidationError> {
    if !path.is_file() {
        return Err(GodotValidationError::NotAFile(path.to_path_buf()));
    }
    let mut child = Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| GodotValidationError::Spawn {
            path: path.to_path_buf(),
            source,
        })?;
    let stdout_reader = drain_pipe(child.stdout.take().expect("stdout is piped"));
    let stderr_reader = drain_pipe(child.stderr.take().expect("stderr is piped"));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(GodotValidationError::Timeout {
                    path: path.to_path_buf(),
                    timeout_ms: timeout.as_millis(),
                });
            }
            Err(source) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(GodotValidationError::Spawn {
                    path: path.to_path_buf(),
                    source,
                });
            }
        }
    };
    let stdout = collect_pipe(stdout_reader, path)?;
    let stderr = collect_pipe(stderr_reader, path)?;
    let stdout = String::from_utf8_lossy(&stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
    let reported = if stdout.is_empty() { stderr } else { stdout };
    if !status.success() {
        return Err(GodotValidationError::VersionCommandFailed {
            path: path.to_path_buf(),
            exit_code: status.code(),
            output: truncate(&reported, 500),
        });
    }
    let version = reported
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    if !is_supported_version(&version) {
        return Err(GodotValidationError::UnsupportedVersion {
            path: path.to_path_buf(),
            version,
        });
    }
    Ok(GodotInstallation {
        executable: path.to_path_buf(),
        version,
    })
}

fn drain_pipe<R>(mut pipe: R) -> JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn collect_pipe(
    reader: JoinHandle<std::io::Result<Vec<u8>>>,
    path: &Path,
) -> Result<Vec<u8>, GodotValidationError> {
    reader
        .join()
        .map_err(|_| GodotValidationError::Output {
            path: path.to_path_buf(),
            source: std::io::Error::other("Godot output reader thread panicked"),
        })?
        .map_err(|source| GodotValidationError::Output {
            path: path.to_path_buf(),
            source,
        })
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn is_supported_version(version: &str) -> bool {
    version == "4.5.1" || version.starts_with("4.5.1.")
}

#[cfg(test)]
mod tests {
    use super::is_supported_version;

    #[test]
    fn requires_the_exact_godot_4_5_1_release_line() {
        assert!(is_supported_version("4.5.1"));
        assert!(is_supported_version("4.5.1.stable.official.f62fdbde1"));
        assert!(!is_supported_version("4.5.10.stable"));
        assert!(!is_supported_version("4.5.2.stable"));
    }
}
