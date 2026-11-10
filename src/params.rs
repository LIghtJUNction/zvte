use core::{fmt, iter::FusedIterator};

/// Maximum combined count of parameters and colon-separated subparameters.
pub const MAX_PARAMS: usize = 32;

/// A bounded list of CSI/DCS parameters, grouped by semicolons.
///
/// `38:2::255:0:0;1` is exposed as `[38, 2, 0, 255, 0, 0]`, then `[1]`.
/// Empty fields are zero. Decimal values saturate at [`u16::MAX`].
#[derive(Clone, Eq)]
pub struct Params {
    values: [u16; MAX_PARAMS],
    groups: [u8; MAX_PARAMS],
    len: u8,
    group: u8,
}

impl Params {
    pub(crate) const fn new() -> Self {
        Self {
            values: [0; MAX_PARAMS],
            groups: [0; MAX_PARAMS],
            len: 0,
            group: 0,
        }
    }

    /// Total number of numeric entries, including subparameters.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Whether no numeric entry has been stored.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterate over parameters, each represented by its subparameter slice.
    #[inline]
    pub fn iter(&self) -> ParamsIter<'_> {
        ParamsIter {
            params: self,
            pos: 0,
        }
    }

    #[inline]
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.group = 0;
    }

    #[inline]
    pub(crate) fn full(&self) -> bool {
        self.len as usize == MAX_PARAMS
    }

    #[inline]
    pub(crate) fn push(&mut self, value: u16, subparameter: bool) -> bool {
        let i = self.len as usize;
        if i == MAX_PARAMS {
            return false;
        }
        self.values[i] = value;
        self.groups[self.group as usize] = (i + 1 - self.group as usize) as u8;
        self.len += 1;
        if !subparameter {
            self.group = self.len;
        }
        true
    }
}

impl Default for Params {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for Params {
    fn eq(&self, other: &Self) -> bool {
        self.iter().eq(other.iter())
    }
}

impl fmt::Debug for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

impl<'a> IntoIterator for &'a Params {
    type Item = &'a [u16];
    type IntoIter = ParamsIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator over the semicolon-separated groups of a [`Params`] list.
#[derive(Clone, Debug)]
pub struct ParamsIter<'a> {
    params: &'a Params,
    pos: usize,
}

impl<'a> Iterator for ParamsIter<'a> {
    type Item = &'a [u16];
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos == self.params.len() {
            return None;
        }
        let start = self.pos;
        self.pos += self.params.groups[start] as usize;
        Some(&self.params.values[start..self.pos])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.params.len() - self.pos;
        (usize::from(remaining != 0), Some(remaining))
    }
}

impl FusedIterator for ParamsIter<'_> {}
