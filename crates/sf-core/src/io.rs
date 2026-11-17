//! Native StitchFoundry design files (`.sfd`): pretty-printed JSON of [`Design`].
//! Unlike machine formats they keep the editable objects, not just stitches.

use crate::model::{Design, FORMAT_VERSION};

pub const EXTENSION: &str = "sfd";

#[derive(Debug, thiserror::Error)]
pub enum DesignFileError {
    #[error("not a StitchFoundry design: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("design file version {0} is newer than this program supports ({FORMAT_VERSION})")]
    TooNew(u32),
}

pub fn to_json(design: &Design) -> Result<String, DesignFileError> {
    Ok(serde_json::to_string_pretty(design)?)
}

pub fn from_json(s: &str) -> Result<Design, DesignFileError> {
    let d: Design = serde_json::from_str(s)?;
    if d.version > FORMAT_VERSION {
        return Err(DesignFileError::TooNew(d.version));
    }
    Ok(d)
}

pub fn from_bytes(b: &[u8]) -> Result<Design, DesignFileError> {
    let d: Design = serde_json::from_slice(b)?;
    if d.version > FORMAT_VERSION {
        return Err(DesignFileError::TooNew(d.version));
    }
    Ok(d)
}
