use std::io::Write;
use std::os::windows::process::CommandExt;
use std::process::{Command, Output, Stdio};
use std::string::{FromUtf16Error, FromUtf8Error};
use thiserror::Error;
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

#[derive(Debug)]
pub struct WslDistribution {
    pub name: String,
    pub status: String,
    pub version: u32,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("IoError running command: {0}")]
    Io(
        #[source]
        #[from]
        std::io::Error,
    ),
    #[error("Status {code}: {stderr_16:?} / {stderr_8:?} / {stdout_16:?} / {stdout_8:?}")]
    BadStatus {
        code: i32,
        stderr_16: Option<String>,
        stderr_8: Option<String>,
        stdout_16: Option<String>,
        stdout_8: Option<String>,
    },
    #[error("Unreadable utf8: {0}")]
    UnreadableUtf8(
        #[source]
        #[from]
        FromUtf8Error,
    ),
    #[error("Unreadable utf16: {0}")]
    UnreadableUtf16(
        #[source]
        #[from]
        FromUtf16Error,
    ),
    #[error("Couldn't parse output of wsl list command")]
    UnexpectedListOutput,
}

fn to_u16(original: &[u8]) -> Vec<u16> {
    original
        .chunks_exact(2)
        .map(|a| u16::from_ne_bytes([a[0], a[1]]))
        .collect()
}

pub fn get_distributions() -> Result<Vec<WslDistribution>, Error> {
    let output = Command::new("wsl.exe")
        .creation_flags(CREATE_NO_WINDOW.0)
        .arg("--list")
        .arg("--verbose")
        .output()?;
    check_wsl_output(&output)?;
    let stdout = String::from_utf16(&to_u16(&output.stdout))?;
    let dist = stdout
        .lines()
        .skip(1)
        .map(|mut line| {
            if line.starts_with('*') {
                line = &line[1..];
            }
            let components = line.split_whitespace().collect::<Vec<_>>();
            if components.len() != 3 {
                return Err(Error::UnexpectedListOutput);
            }
            Ok(WslDistribution {
                name: components[0].to_string(),
                status: components[1].to_string(),
                version: components[2]
                    .parse()
                    .map_err(|_| Error::UnexpectedListOutput)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(dist)
}

fn check_wsl_output(output: &Output) -> Result<(), Error> {
    if !output.status.success() {
        // Depending on if the error is in WSL or the distribution we may get different encodings
        // back - collect them all for debugging
        let stderr_16 = String::from_utf16(&to_u16(&output.stderr)).ok();
        let stderr_8 = String::from_utf8(output.stderr.clone()).ok();
        let stdout_16 = String::from_utf16(&to_u16(&output.stdout)).ok();
        let stdout_8 = String::from_utf8(output.stdout.clone()).ok();
        return Err(Error::BadStatus {
            code: output.status.code().unwrap_or_default(),
            stderr_16,
            stderr_8,
            stdout_16,
            stdout_8,
        });
    }
    Ok(())
}

impl WslDistribution {
    pub fn read_file(&self, path: &str) -> Result<String, Error> {
        let output = Command::new("wsl.exe")
            .creation_flags(CREATE_NO_WINDOW.0)
            .arg("--distribution")
            .arg(&self.name)
            .arg("--user")
            .arg("root")
            .arg("cat")
            .arg(path)
            .output()?;
        check_wsl_output(&output)?;
        Ok(String::from_utf8(output.stdout)?)
    }

    pub fn set_read_only(&self, path: &str, read_only: bool) -> Result<(), Error> {
        let arg = if read_only { "+i" } else { "-i" };
        let output = Command::new("wsl.exe")
            .creation_flags(CREATE_NO_WINDOW.0)
            .arg("--distribution")
            .arg(&self.name)
            .arg("--user")
            .arg("root")
            .arg("chattr")
            .arg(arg)
            .arg(path)
            .output()?;
        check_wsl_output(&output)?;
        Ok(())
    }

    pub fn write_file(&self, path: &str, contents: &str) -> Result<(), Error> {
        let mut p = Command::new("wsl.exe")
            .creation_flags(CREATE_NO_WINDOW.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("--distribution")
            .arg(&self.name)
            .arg("--user")
            .arg("root")
            .arg("tee")
            .arg(path)
            .spawn()?;

        let mut child_stdin = p.stdin.take().unwrap();
        child_stdin.write_all(contents.as_bytes())?;
        drop(child_stdin);

        let output = p.wait_with_output()?;
        check_wsl_output(&output)?;
        Ok(())
    }

    pub fn terminate(&self) -> Result<(), Error> {
        let output = Command::new("wsl.exe")
            .creation_flags(CREATE_NO_WINDOW.0)
            .arg("--terminate")
            .arg(&self.name)
            .output()?;
        check_wsl_output(&output)?;
        Ok(())
    }

    pub fn was_stopped(&self) -> bool {
        self.status == "Stopped"
    }
}
