// Copyright 2022 The ChromiumOS Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign};

use crate::constants;

/// Usage hint flags structure.
#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct UsageHints(u32);

impl UsageHints {
    /// Returns usage hint flags value as 32-bit integer.
    pub fn bits(self) -> u32 {
        self.0
    }
}

impl BitOr<UsageHint> for UsageHints {
    type Output = Self;

    fn bitor(self, rhs: UsageHint) -> Self {
        Self(self.0 | rhs.bits())
    }
}

impl BitOrAssign<UsageHint> for UsageHints {
    fn bitor_assign(&mut self, rhs: UsageHint) {
        self.0 |= rhs.bits();
    }
}

impl BitXor<UsageHint> for UsageHints {
    type Output = Self;

    fn bitxor(self, rhs: UsageHint) -> Self {
        Self(self.0 ^ rhs.bits())
    }
}

impl BitXorAssign<UsageHint> for UsageHints {
    fn bitxor_assign(&mut self, rhs: UsageHint) {
        self.0 ^= rhs.bits();
    }
}

impl BitAnd<UsageHint> for UsageHints {
    type Output = Self;

    fn bitand(self, rhs: UsageHint) -> Self {
        Self(self.0 & rhs.bits())
    }
}

impl BitAndAssign<UsageHint> for UsageHints {
    fn bitand_assign(&mut self, rhs: UsageHint) {
        self.0 &= rhs.bits();
    }
}

/// Gives the driver a hint of intended usage to optimize allocation (e.g. tiling).
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UsageHint {
    /// Surface usage not indicated.
    Generic = constants::VA_SURFACE_ATTRIB_USAGE_HINT_GENERIC,
    /// Surface used by video decoder.
    Decoder = constants::VA_SURFACE_ATTRIB_USAGE_HINT_DECODER,
    /// Surface used by video encoder.
    Encoder = constants::VA_SURFACE_ATTRIB_USAGE_HINT_ENCODER,
    /// Surface read by video post-processing.
    VppRead = constants::VA_SURFACE_ATTRIB_USAGE_HINT_VPP_READ,
    /// Surface written by video post-processing.
    VppWrite = constants::VA_SURFACE_ATTRIB_USAGE_HINT_VPP_WRITE,
    /// Surface used for display.
    Display = constants::VA_SURFACE_ATTRIB_USAGE_HINT_DISPLAY,
    /// Surface used for export to third-party APIs, e.g. via `vaExportSurfaceHandle()`.
    Export = constants::VA_SURFACE_ATTRIB_USAGE_HINT_EXPORT,
}

impl From<UsageHint> for UsageHints {
    fn from(val: UsageHint) -> Self {
        Self(val.bits())
    }
}

impl From<UsageHint> for u32 {
    fn from(val: UsageHint) -> u32 {
        val.bits()
    }
}

impl UsageHint {
    pub(crate) fn bits(self) -> u32 {
        self as u32
    }
}
