#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct U24([u8; 3]);

impl U24 {
    pub fn new(value: u32) -> Self {
        assert!(value <= 0xFFFFFF, "Value out of range for U24");
        Self([
            (value & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            ((value >> 16) & 0xFF) as u8,
        ])
    }

    pub fn as_u32(&self) -> u32 {
        (self.0[0] as u32) | ((self.0[1] as u32) << 8) | ((self.0[2] as u32) << 16)
    }

    pub fn as_bytes(&self) -> [u8; 3] {
        self.0
    }

    pub fn to_be_bytes(&self) -> [u8; 3] {
        [self.0[2], self.0[1], self.0[0]]
    }
}

impl From<u32> for U24 {
    fn from(value: u32) -> Self {
        U24::new(value)
    }
}

impl From<U24> for u32 {
    fn from(value: U24) -> u32 {
        value.as_u32()
    }
}

use std::fmt;

impl fmt::Display for U24 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_u32())
    }
}
