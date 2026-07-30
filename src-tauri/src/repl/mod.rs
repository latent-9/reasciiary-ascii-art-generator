//! The command language behind the fake terminal.
//!
//! Nothing here spawns a shell. The window looks like a terminal because that
//! is the right shape for this tool, not because a PTY is hiding behind it.
//!
//! ```text
//! ascii logo.txt --depth 12 | crt --curve 0.2 > out.mp4
//! ```

use std::collections::HashMap;

use crate::art::params::Params;

/// One segment between pipes: a generator if it is first, a filter otherwise.
#[derive(Clone, Debug)]
pub struct Stage {
    pub name: String,
    pub params: Params,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub stages: Vec<Stage>,
    /// The path after `>`. Its extension picks the export format.
    pub output: Option<String>,
}

pub fn parse(line: &str) -> Result<Command, String> {
    let tokens = tokenize(line);
    if tokens.is_empty() {
        return Err("nothing to run".into());
    }

    let (body, output) = match tokens.iter().position(|token| token == ">") {
        None => (tokens.as_slice(), None),
        Some(index) => {
            let path = tokens
                .get(index + 1)
                .ok_or("`>` needs a file to write to")?
                .clone();
            if tokens.len() > index + 2 {
                return Err("only one file may follow `>`".into());
            }
            (&tokens[..index], Some(path))
        }
    };

    // `> out.mp4` on its own reaches the split below as one empty segment and
    // comes back complaining about pipes, which is not what went wrong.
    if body.is_empty() {
        return Err("nothing to run".into());
    }

    let mut stages = Vec::new();
    for segment in body.split(|token| token == "|") {
        let (name, rest) = segment
            .split_first()
            .ok_or("a pipe segment is empty — two `|` in a row?")?;
        stages.push(Stage {
            name: name.clone(),
            params: read_params(rest)?,
        });
    }

    Ok(Command { stages, output })
}

/// Splits on whitespace, keeping anything inside double quotes together so a
/// path with a space in it survives.
fn tokenize(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg(test)]
mod tests;

fn read_params(tokens: &[String]) -> Result<Params, String> {
    let mut positional = Vec::new();
    let mut flags: HashMap<String, Option<String>> = HashMap::new();

    let mut index = 0;
    while index < tokens.len() {
        let token = &tokens[index];
        match token.strip_prefix("--") {
            None => positional.push(token.clone()),
            Some(name) => {
                if name.is_empty() {
                    return Err("`--` on its own is not a flag".into());
                }
                // A flag takes the next token as its value unless that token is
                // itself a flag, which is what makes `--still` work without one.
                let value = tokens
                    .get(index + 1)
                    .filter(|next| !next.starts_with("--"))
                    .cloned();
                if value.is_some() {
                    index += 1;
                }
                flags.insert(name.to_string(), value);
            }
        }
        index += 1;
    }

    Ok(Params { positional, flags })
}
