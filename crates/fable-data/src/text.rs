//! The `Key value;` text format shared by `.wld` and `.tng` files.

use crate::{Error, Result};

/// One `Key[arg] value;` statement.
pub(crate) struct Statement<'a> {
    pub line: usize,
    pub key: &'a str,
    pub arg: Option<&'a str>,
    pub value: &'a str,
}

impl Statement<'_> {
    pub fn err(&self, reason: impl Into<String>) -> Error {
        Error::Text {
            line: self.line,
            reason: reason.into(),
        }
    }

    pub fn string(&self) -> Result<String> {
        self.value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .map(str::to_owned)
            .ok_or_else(|| self.err(format!("{}: expected a quoted string", self.key)))
    }

    pub fn number<T: std::str::FromStr>(&self) -> Result<T> {
        self.value
            .parse()
            .map_err(|_| self.err(format!("{}: bad number {:?}", self.key, self.value)))
    }

    pub fn bool(&self) -> Result<bool> {
        match self.value {
            "TRUE" => Ok(true),
            "FALSE" => Ok(false),
            v => Err(self.err(format!("{}: expected TRUE or FALSE, found {v:?}", self.key))),
        }
    }
}

pub(crate) fn statements(text: &str) -> Result<Vec<Statement<'_>>> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let body = raw.trim();
        if body.is_empty() {
            continue;
        }
        let body = body.strip_suffix(';').ok_or_else(|| Error::Text {
            line,
            reason: format!("missing ';' in {body:?}"),
        })?;
        let (head, value) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        // A trailing `[arg]` is an argument (`MiniMapRegionExitTextOffsetX[PicnicArea]`);
        // brackets elsewhere are part of the key (`KeyCameras[0].Position`).
        let (key, arg) = match head.strip_suffix(']').and_then(|h| h.split_once('[')) {
            Some((key, arg)) => (key, Some(arg)),
            None => (head, None),
        };
        out.push(Statement {
            line,
            key,
            arg,
            value: value.trim(),
        });
    }
    Ok(out)
}
