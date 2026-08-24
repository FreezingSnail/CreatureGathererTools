use serde::Serialize;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;

pub const PAGE_SIZE: u32 = 256;
pub const SAVE_SECTOR: u32 = 4096;
pub const FLASH_PAGES: u32 = 65536;

const MAX_ADDRESS: u32 = 0x00ff_ffff;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum FieldSource {
    Raw {
        path: PathBuf,
    },
    CArray {
        path: PathBuf,
    },
    Symbol {
        path: PathBuf,
    },
    Image {
        path: PathBuf,
    },
    Builder {
        key: String,
    },
    Save,
    /// Layout-only field with no pack-spec source.
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Bytes,
    Image {
        frames: u16,
        width: u16,
        height: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    pub name: String,
    /// Optional `::`-separated C++ namespace path for the public field name.
    pub namespace: Option<String>,
    pub addr: u32,
    /// Number of source bytes written for this field, excluding alignment padding.
    pub source_len: u32,
    /// Address span from this field to the next field in its section, including padding.
    pub span: u32,
    pub kind: FieldKind,
    pub in_save: bool,
    /// The layout-spec source that produced this field.
    pub source: FieldSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Offsets {
    pub data_len: u32,
    pub data_len_flash: u32,
    pub save_len: u32,
    pub save_len_flash: u32,
    pub data_page: u16,
    pub save_page: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    SaveAlreadyBegun,
    SaveNotBegun,
    SaveOffsetOutOfRange(u32),
    SaveOffsetUnaligned(u32),
    UnterminatedNamespace(String),
    AddressOverflow(u32),
    NamespaceNotOpen,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SaveAlreadyBegun => write!(f, "save section has already begun"),
            Self::SaveNotBegun => write!(f, "save section has not begun"),
            Self::SaveOffsetOutOfRange(offset) => {
                write!(
                    f,
                    "save offset 0x{offset:06X} is outside the reserved save region"
                )
            }
            Self::SaveOffsetUnaligned(offset) => {
                write!(f, "save offset 0x{offset:06X} is not sector-aligned")
            }
            Self::UnterminatedNamespace(name) => write!(f, "namespace {name:?} is still open"),
            Self::AddressOverflow(address) => {
                write!(f, "FX address 0x{address:06X} exceeds 24 bits")
            }
            Self::NamespaceNotOpen => write!(f, "no namespace is open"),
        }
    }
}

impl Error for LayoutError {}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    bytes: Vec<u8>,
    fields: Vec<Field>,
    namespace: Option<String>,
    save_start: Option<usize>,
    save_sectors: Option<u32>,
    data_len: Option<usize>,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            fields: Vec::new(),
            namespace: None,
            save_start: None,
            save_sectors: None,
            data_len: None,
        }
    }

    /// Begin assigning fields to a `::`-separated C++ namespace path.
    ///
    /// Layout specs validate path components before packing; this low-level API
    /// retains the supplied path so programmatic callers preserve compatibility.
    pub fn begin_namespace(&mut self, name: &str) {
        self.namespace = Some(name.to_owned());
    }

    pub fn end_namespace(&mut self) -> Result<(), LayoutError> {
        if self.namespace.take().is_none() {
            return Err(LayoutError::NamespaceNotOpen);
        }
        Ok(())
    }

    pub fn field(&mut self, name: &str, kind: FieldKind) -> Result<u32, LayoutError> {
        self.field_with_source(name, kind, FieldSource::Unknown)
    }

    pub fn field_with_source(
        &mut self,
        name: &str,
        kind: FieldKind,
        source: FieldSource,
    ) -> Result<u32, LayoutError> {
        self.backfill_previous_field();
        let addr = self.address();
        if addr > MAX_ADDRESS {
            return Err(LayoutError::AddressOverflow(addr));
        }
        self.fields.push(Field {
            name: name.to_owned(),
            namespace: self.namespace.clone(),
            addr,
            source_len: 0,
            span: 0,
            kind,
            in_save: self.save_start.is_some(),
            source,
        });
        Ok(addr)
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), LayoutError> {
        if let Some(last_index) = bytes.len().checked_sub(1) {
            let last_index = u32::try_from(last_index).unwrap_or(u32::MAX);
            let last_address = self.address().saturating_add(last_index);
            if last_address > MAX_ADDRESS {
                return Err(LayoutError::AddressOverflow(last_address));
            }
        }
        self.bytes.extend_from_slice(bytes);
        if let Some(field) = self.fields.last_mut()
            && field.in_save == self.save_start.is_some()
        {
            field.source_len = field.source_len.saturating_add(bytes.len() as u32);
        }
        Ok(())
    }

    pub fn pad(&mut self, align: u32, force: bool) -> u32 {
        if align == 0 {
            return 0;
        }
        let remainder = self.address() % align;
        let written = if remainder == 0 {
            if force { align } else { 0 }
        } else {
            align - remainder
        };
        self.bytes.resize(self.bytes.len() + written as usize, 0xff);
        written
    }

    pub fn begin_save(&mut self) -> Result<(), LayoutError> {
        if self.save_start.is_some() {
            return Err(LayoutError::SaveAlreadyBegun);
        }
        self.data_len = Some(self.bytes.len());
        self.pad(PAGE_SIZE, false);
        self.save_start = Some(self.bytes.len());
        Ok(())
    }

    /// Reserve sectors * 4096 bytes of erased save region. Must call begin_save() first.
    pub fn reserve_save_sectors(&mut self, sectors: u8) -> Result<(), LayoutError> {
        if self.save_start.is_none() {
            return Err(LayoutError::SaveNotBegun);
        }
        let total = u32::from(sectors) * SAVE_SECTOR;
        self.save_sectors = Some(total);
        self.bytes.resize(self.bytes.len() + total as usize, 0xff);
        Ok(())
    }

    /// Record a zero-length named field at a save-relative sector offset.
    pub fn save_offset_field(&mut self, name: &str, offset: u32) -> Result<(), LayoutError> {
        if self.save_start.is_none() {
            return Err(LayoutError::SaveNotBegun);
        }
        let total = self.save_sectors.unwrap_or(0);
        if offset >= total {
            return Err(LayoutError::SaveOffsetOutOfRange(offset));
        }
        if offset % SAVE_SECTOR != 0 {
            return Err(LayoutError::SaveOffsetUnaligned(offset));
        }
        if self.fields.last().is_some_and(|field| field.in_save) {
            self.backfill_previous_field_to(offset);
        } else {
            self.backfill_previous_field();
        }
        self.fields.push(Field {
            name: name.to_owned(),
            namespace: None,
            addr: offset,
            source_len: 0,
            span: 0,
            kind: FieldKind::Bytes,
            in_save: true,
            source: FieldSource::Save,
        });
        Ok(())
    }

    pub fn address(&self) -> u32 {
        let section_start = self.save_start.unwrap_or(0);
        (self.bytes.len() - section_start) as u32
    }

    pub fn fields(&self) -> &[Field] {
        &self.fields
    }

    pub fn finalize(self) -> Result<(Offsets, Vec<u8>), LayoutError> {
        self.finalize_with_fields()
            .map(|(_, offsets, bytes)| (offsets, bytes))
    }

    /// Finalize the byte layout while retaining fields with their completed spans.
    pub fn finalize_with_fields(mut self) -> Result<(Vec<Field>, Offsets, Vec<u8>), LayoutError> {
        if let Some(namespace) = self.namespace {
            return Err(LayoutError::UnterminatedNamespace(namespace));
        }
        self.backfill_previous_field();

        let data_len = self.data_len.unwrap_or(self.bytes.len());
        let (data_len_flash, save_len, save_len_flash) = if let Some(save_start) = self.save_start {
            let save_len = self.bytes.len() - save_start;
            self.pad(SAVE_SECTOR, self.save_sectors.is_none());
            (save_start, save_len, self.bytes.len() - save_start)
        } else {
            self.pad(PAGE_SIZE, false);
            (self.bytes.len(), 0, 0)
        };

        let data_len = Self::length_u32(data_len)?;
        let data_len_flash = Self::length_u32(data_len_flash)?;
        let save_len = Self::length_u32(save_len)?;
        let save_len_flash = Self::length_u32(save_len_flash)?;
        let data_pages = data_len_flash / PAGE_SIZE;
        let save_pages = save_len_flash / PAGE_SIZE;
        let used_pages = data_pages.saturating_add(save_pages);
        if used_pages > FLASH_PAGES {
            return Err(LayoutError::AddressOverflow(MAX_ADDRESS + 1));
        }
        let save_page = self.save_start.map(|_| (FLASH_PAGES - save_pages) as u16);

        Ok((
            self.fields,
            Offsets {
                data_len,
                data_len_flash,
                save_len,
                save_len_flash,
                data_page: (FLASH_PAGES - used_pages) as u16,
                save_page,
            },
            self.bytes,
        ))
    }

    fn backfill_previous_field(&mut self) {
        let section_len = match self.fields.last() {
            Some(previous) if previous.in_save => {
                self.bytes.len() - self.save_start.expect("save fields require a save section")
            }
            Some(_) => self.data_len.unwrap_or(self.bytes.len()),
            None => return,
        };
        self.backfill_previous_field_to(section_len as u32);
    }

    fn backfill_previous_field_to(&mut self, end: u32) {
        let Some(previous) = self.fields.last_mut() else {
            return;
        };
        previous.span = end.saturating_sub(previous.addr);
    }

    fn length_u32(length: usize) -> Result<u32, LayoutError> {
        if length > MAX_ADDRESS as usize + 1 {
            return Err(LayoutError::AddressOverflow(MAX_ADDRESS + 1));
        }
        Ok(length as u32)
    }
}
