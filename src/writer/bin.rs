//! Dump raw VM bytecode blob (stub).

use crate::model::ProcessedProject;
use crate::writer::u24::U24;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

pub fn emit(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    scripts(project, out_dir)?;
    map(project, out_dir)?;
    raw_map(project, out_dir)?;
    text(project, out_dir)?;
    Ok(())
}

fn scripts(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("scripts.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    for blob_chunk in &project.blob.blob {
        // Write the actual blob data
        writer.write_all(&blob_chunk.blob)?;

        // Calculate padding needed for 128-byte alignment
        let padding = 128 - (blob_chunk.blob.len() % 128);
        if padding < 128 {
            // Write all padding bytes at once instead of one by one
            let padding_bytes = vec![0u8; padding];
            writer.write_all(&padding_bytes)?;
        }
    }

    writer.flush()?;
    Ok(())
}

fn raw_map(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("raw_map.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    for value in &project.raw_map {
        writer.write_all(&value.to_le_bytes())?;
    }
    Ok(())
}

fn map(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("map.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    for chunk in &project.map {
        // Write the actual blob data
        for &value in chunk {
            writer.write_all(&value.to_be_bytes())?
        }
    }

    writer.flush()?;
    Ok(())
}

fn text(project: &ProcessedProject, out_dir: &Path) -> io::Result<()> {
    let path = out_dir.join("text.bin");
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    let mut sorted_texts: Vec<(String, u16)> =
        project.texts.iter().map(|(k, v)| (k.clone(), *v)).collect();

    sorted_texts.sort_by(|a, b| a.1.cmp(&b.1));

    let mut text_offsets: Vec<u16> = Vec::new(); // Initial offset is 0
    let mut text_blob: Vec<u8> = Vec::new();

    let mut cursor: u16 = 0;
    for (text, _index) in &sorted_texts {
        text_offsets.push(cursor);
        let ascii: Vec<u8> = text.bytes().collect();
        let len_bytes: [u8; 2] = (ascii.len() as u16).to_le_bytes();
        text_blob.extend_from_slice(&len_bytes);
        text_blob.extend_from_slice(&ascii);
        cursor += (2 + ascii.len()) as u16;
    }
    let offset_size = (text_offsets.len()) as u16;
    writer.write_all(&offset_size.to_le_bytes())?;
    for value in &text_offsets {
        println!("{:?}", value);
        writer.write_all(&value.to_le_bytes())?
    }
    println!("{:?}", text_blob);
    writer.write_all(&text_blob)?;

    writer.flush()
}
