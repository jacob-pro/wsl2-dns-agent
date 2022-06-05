use std::process::Command;
use std::str::Utf8Error;
use std::string::FromUtf16Error;
use thiserror::Error;

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
    #[error("Status {code}, stderr: {stderr}")]
    BadStatus { code: i32, stderr: String },
    #[error("Unreadable utf8: {0}")]
    UnreadableUtf8(
        #[source]
        #[from]
        Utf8Error,
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
        .into_iter()
        .map(|a| u16::from_ne_bytes([a[0], a[1]]))
        .collect()
}

pub fn get_distributions() -> Result<Vec<WslDistribution>, Error> {
    let output = Command::new("wsl.exe")
        .arg("--list")
        .arg("--verbose")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf16(&to_u16(&output.stderr))?;
        return Err(Error::BadStatus {
            code: output.status.code().unwrap_or_default(),
            stderr,
        });
    }
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

impl WslDistribution {
    pub fn read_wsl_conf(&self) {
        // let output = Command::new("wsl.exe")
        //     .arg("--distribution")
        //     .arg(&self.name)
        //     .arg("cat")
        //     .arg("/etc/wsl.conf")
        //     .output()?;
    }
}
