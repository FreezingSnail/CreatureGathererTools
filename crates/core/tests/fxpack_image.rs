use creaturevm::fxpack::{image, layout::Layout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let suffix = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "creaturevm-fxpack-image-{}-{suffix}",
            std::process::id()
        ));
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn data_only_fixture() -> (creaturevm::fxpack::layout::Offsets, Vec<u8>) {
    let mut layout = Layout::new();
    layout.write(&[0x10, 0x20, 0x30]).unwrap();
    layout.finalize().unwrap()
}

fn data_plus_save_layout() -> (creaturevm::fxpack::layout::Offsets, Vec<u8>) {
    let mut layout = Layout::new();
    layout.write(&[0x10, 0x20, 0x30]).unwrap();
    layout.begin_save().unwrap();
    layout.write(&[0xa0, 0xb0, 0xc0, 0xd0]).unwrap();
    layout.finalize().unwrap()
}

#[test]
fn data_only_layout() {
    let (offsets, bytes) = data_only_fixture();
    let output = TestDir::new();
    let artifacts = image::write(&bytes, &offsets, output.path(), "fxdata").unwrap();

    assert!(artifacts.save_bin.is_none());
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 2);
    assert_eq!(
        std::fs::read(&artifacts.data_bin).unwrap(),
        &[0x10, 0x20, 0x30]
    );

    let dev = std::fs::read(&artifacts.dev_bin).unwrap();
    assert_eq!(dev.len(), offsets.data_len_flash as usize);
    assert_eq!(&dev[..offsets.data_len as usize], &[0x10, 0x20, 0x30]);
    assert!(
        dev[offsets.data_len as usize..]
            .iter()
            .all(|byte| *byte == 0xff)
    );
}

#[test]
fn data_plus_save() {
    let (offsets, bytes) = data_plus_save_layout();
    let output = TestDir::new();
    let artifacts = image::write(&bytes, &offsets, output.path(), "fxdata").unwrap();

    let save_bin = artifacts.save_bin.as_ref().unwrap();
    assert_eq!(std::fs::read_dir(output.path()).unwrap().count(), 3);
    assert_eq!(
        std::fs::read(&artifacts.data_bin).unwrap(),
        &[0x10, 0x20, 0x30]
    );
    assert_eq!(std::fs::read(save_bin).unwrap(), &[0xa0, 0xb0, 0xc0, 0xd0]);

    let dev = std::fs::read(&artifacts.dev_bin).unwrap();
    assert_eq!(
        dev.len(),
        offsets.data_len_flash as usize + offsets.save_len_flash as usize
    );
    assert_eq!(
        &dev[offsets.data_len_flash as usize..][..offsets.save_len as usize],
        &[0xa0, 0xb0, 0xc0, 0xd0]
    );
}

#[test]
fn truncated_buffer_errors() {
    let (offsets, bytes) = data_plus_save_layout();
    let output = TestDir::new();
    let result = image::write(&bytes[..bytes.len() - 1], &offsets, output.path(), "fxdata");

    let error = match result {
        Ok(_) => panic!("truncated buffer unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("byte buffer is"));
    assert!(!output.path().exists());
}

#[test]
fn pad_bytes_are_0xff() {
    let (offsets, bytes) = data_plus_save_layout();
    let output = TestDir::new();
    let artifacts = image::write(&bytes, &offsets, output.path(), "fxdata").unwrap();
    let dev = std::fs::read(&artifacts.dev_bin).unwrap();

    assert!(
        dev[offsets.data_len as usize..offsets.data_len_flash as usize]
            .iter()
            .all(|byte| *byte == 0xff)
    );
    let save_end = offsets.data_len_flash as usize + offsets.save_len as usize;
    assert!(dev[save_end..].iter().all(|byte| *byte == 0xff));
}
