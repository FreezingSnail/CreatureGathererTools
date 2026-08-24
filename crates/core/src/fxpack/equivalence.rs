use anyhow::bail;
use std::path::Path;

use super::{
    layout::Field,
    pack::{self, PackPlan},
    spec::LayoutSpec,
};

/// Assert two layout plans preserve every published field and packed byte.
pub fn assert_equivalent(old: &PackPlan, new: &PackPlan) -> anyhow::Result<()> {
    let shared_fields = old.fields.len().min(new.fields.len());
    for index in 0..shared_fields {
        if old.fields[index] != new.fields[index] {
            bail!(
                "layout equivalence: field {index} differs\n  old: {}\n  new: {}",
                describe_field(&old.fields[index]),
                describe_field(&new.fields[index]),
            );
        }
    }
    if old.fields.len() != new.fields.len() {
        bail!(
            "layout equivalence: field {shared_fields} differs\n  old: {}\n  new: {}",
            old.fields
                .get(shared_fields)
                .map(describe_field)
                .unwrap_or_else(|| "<missing>".to_owned()),
            new.fields
                .get(shared_fields)
                .map(describe_field)
                .unwrap_or_else(|| "<missing>".to_owned()),
        );
    }

    let shared_bytes = old.bytes.len().min(new.bytes.len());
    for index in 0..shared_bytes {
        if old.bytes[index] != new.bytes[index] {
            bail!(
                "layout equivalence: byte 0x{index:06X} differs: old=0x{:02X}, new=0x{:02X}",
                old.bytes[index],
                new.bytes[index],
            );
        }
    }
    if old.bytes.len() != new.bytes.len() {
        bail!(
            "layout equivalence: byte length differs: old={}, new={}",
            old.bytes.len(),
            new.bytes.len(),
        );
    }
    Ok(())
}

/// Load and compare two complete file layouts without producing artifacts.
pub fn assert_layout_files_equivalent(old_path: &Path, new_path: &Path) -> anyhow::Result<()> {
    let old = LayoutSpec::load(old_path)?;
    let new = LayoutSpec::load(new_path)?;
    let old_plan = pack::plan(&old, &[])?;
    let new_plan = pack::plan(&new, &[])?;
    assert_equivalent(&old_plan, &new_plan)
}

fn describe_field(field: &Field) -> String {
    format!(
        "name={:?}, namespace={:?}, addr=0x{:06X}, source_len={}, span={}, kind={:?}, in_save={}, source={:?}",
        field.name,
        field.namespace,
        field.addr,
        field.source_len,
        field.span,
        field.kind,
        field.in_save,
        field.source,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxpack::layout::{FieldKind, FieldSource, Offsets};
    use std::path::PathBuf;

    fn plan(name: &str, addr: u32, byte: u8) -> PackPlan {
        PackPlan {
            fields: vec![Field {
                name: name.to_owned(),
                namespace: None,
                addr,
                source_len: 1,
                span: 1,
                kind: FieldKind::Bytes,
                in_save: false,
                source: FieldSource::Unknown,
            }],
            offsets: Offsets {
                data_len: 1,
                data_len_flash: 256,
                save_len: 0,
                save_len_flash: 0,
                data_page: 0xffff,
                save_page: None,
            },
            bytes: vec![byte],
        }
    }

    #[test]
    fn perturbed_layout_fixture_reports_first_field_and_both_addresses() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/testdata/fxpack/pack");
        let old = LayoutSpec::load(&root.join("expand_equivalent_layout.toml")).unwrap();
        let perturbed = LayoutSpec::load(&root.join("expand_perturbed_layout.toml")).unwrap();
        let error = assert_equivalent(
            &pack::plan(&old, &[]).unwrap(),
            &pack::plan(&perturbed, &[]).unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("field 2 differs"));
        assert!(error.contains("old: name=\"refs\""));
        assert!(error.contains("new: name=\"changed\""));
        assert_eq!(error.matches("addr=0x000002").count(), 2);
    }

    #[test]
    fn field_difference_reports_first_field_and_both_addresses() {
        let error = assert_equivalent(&plan("old", 0x12, 0x10), &plan("new", 0x34, 0x10))
            .unwrap_err()
            .to_string();
        assert!(error.contains("field 0 differs"));
        assert!(error.contains("old: name=\"old\""));
        assert!(error.contains("new: name=\"new\""));
        assert!(error.contains("addr=0x000012"));
        assert!(error.contains("addr=0x000034"));
    }

    #[test]
    fn byte_difference_reports_first_address_and_values() {
        let error = assert_equivalent(&plan("same", 0, 0x10), &plan("same", 0, 0x20))
            .unwrap_err()
            .to_string();
        assert!(error.contains("byte 0x000000 differs"));
        assert!(error.contains("old=0x10"));
        assert!(error.contains("new=0x20"));
    }
}
