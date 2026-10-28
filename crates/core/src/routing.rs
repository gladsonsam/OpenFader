//! Stable, non-strip patch guards. Only independent provider state is writable.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteState {
    pub source: String,
    pub revision: u64,
    pub generation: u64,
}

/// Independently captured tap and source context for one physical output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputTapState {
    pub target: String,
    pub source: String,
    pub tap: u8,
}

#[derive(Clone, Debug)]
pub struct OutputPlane {
    pub key: &'static str,
    pub label: &'static str,
    pub count: u16,
    pub clearable: bool,
    pub sources: Vec<(&'static str, &'static str, usize)>,
}
