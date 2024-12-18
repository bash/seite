use anyhow::{anyhow, Context as _, Result};
use pulldown_cmark::CowStr;
use std::io::Write as _;
use std::process::{Command, Stdio};

pub(crate) fn highlight_code(command: &str, language: CowStr<'_>, code: String) -> Result<String> {
    let mut words: Vec<_> = shell_words::split(command)?;
    replace_language_placeholder(&mut words, language);

    let mut child = Command::new(words.first().context("command is empty")?)
        .args(words.iter().skip(1))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    write!(child.stdin.take().expect("piped"), "{}", code)?;

    let output = child.wait_with_output()?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?)
    } else {
        Err(anyhow!(
            "{} exited with non-zero exit code: {}",
            shell_words::join(words),
            output.status
        ))
    }
}

fn replace_language_placeholder(words: &mut Vec<String>, language: CowStr<'_>) {
    for word in words {
        if word == "{}" {
            *word = language.to_string();
        }
    }
}
