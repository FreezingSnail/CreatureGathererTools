use anyhow::{Context, bail};
use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use super::{
    font::Atlas,
    sprite::{self, Params},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default, rename = "set")]
    sets: Vec<Set>,
    strings: Option<Strings>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Set {
    output: PathBuf,
    images: Vec<PathBuf>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Strings {
    output: PathBuf,
    font: PathBuf,
    input: PathBuf,
    glyph_width: u16,
    glyph_height: u16,
    #[serde(default = "default_shades")]
    shades: u8,
    order: Vec<String>,
}

fn default_shades() -> u8 {
    4
}

/// Emit legacy `Sprites.txt` C arrays from PNG/font sources without Python.
pub fn emit(config_path: &Path) -> anyhow::Result<()> {
    let contents = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading sprite source config {}", config_path.display()))?;
    let config: Config = toml::from_str(&contents)
        .with_context(|| format!("parsing sprite source config {}", config_path.display()))?;
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));

    for set in config.sets {
        let mut output = String::new();
        for image in set.images {
            let image = resolve(base, image);
            let name = image_name(&image)?;
            let (width, height) = image_dimensions(&image)?;
            let sheet = sprite::encode_png_legacy(
                &image,
                Params {
                    width,
                    height,
                    shades: 4,
                    spacing: 0,
                },
            )?;
            output.push_str(&format_array(&name, &sheet.bytes));
        }
        write_output(&resolve(base, set.output), output)?;
    }

    if let Some(strings) = config.strings {
        emit_strings(base, strings)?;
    }
    Ok(())
}

fn emit_strings(base: &Path, config: Strings) -> anyhow::Result<()> {
    if !(2..=4).contains(&config.shades) {
        bail!("string image shades must be 2, 3, or 4");
    }
    let input = resolve(base, config.input);
    let namespaces = parse_legacy_strings(
        &std::fs::read_to_string(&input)
            .with_context(|| format!("reading string source {}", input.display()))?,
    )?;
    let strings: Vec<(String, String)> = namespaces
        .iter()
        .flat_map(|(_, values)| values.clone())
        .collect();
    let names: HashSet<&str> = strings.iter().map(|(name, _)| name.as_str()).collect();
    if names.len() != strings.len() {
        bail!("duplicate string identifier in {}", input.display());
    }
    if config.order.len() != strings.len()
        || config.order.iter().collect::<HashSet<_>>().len() != config.order.len()
        || config
            .order
            .iter()
            .any(|name| !names.contains(name.as_str()))
    {
        bail!(
            "string image order must contain every identifier from {} exactly once",
            input.display()
        );
    }

    let atlas_path = resolve(base, config.font);
    let atlas = Atlas::load(&atlas_path, config.glyph_width, config.glyph_height, 0)?;
    let by_name: std::collections::BTreeMap<_, _> = strings
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let mut output = String::new();
    for name in &config.order {
        let sheet = super::font::render_strings(
            &atlas,
            &[(name.clone(), by_name[name.as_str()].to_owned())],
            config.shades,
        )?
        .pop()
        .expect("one input string produces one image");
        output.push_str(&format_array(name, &sheet.sheet.bytes));
    }
    output.push('\n');
    for (index, (namespace, values)) in namespaces.into_iter().enumerate() {
        if index > 0 {
            output.push('\n');
        }
        output.push_str(&format!(
            "namespace {namespace} {{\nuint24_t  {namespace}[] = {{{}}};\n}} namespace_end",
            values
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    write_output(&resolve(base, config.output), output)
}

fn parse_legacy_strings(content: &str) -> anyhow::Result<Vec<(String, Vec<(String, String)>)>> {
    let mut namespaces = Vec::new();
    let mut rest = content;
    while let Some(namespace_start) = rest.find("namespace ") {
        rest = &rest[namespace_start + "namespace ".len()..];
        let Some(open_brace) = rest.find('{') else {
            break;
        };
        let namespace = rest[..open_brace].trim();
        rest = &rest[open_brace + 1..];
        let Some(close_brace) = rest.find('}') else {
            break;
        };
        let block = &rest[..close_brace];
        rest = &rest[close_brace + 1..];
        let mut values = Vec::new();
        for line in block.trim().split('\n').filter(|line| !line.is_empty()) {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            values.push((key.replace(' ', ""), value.replace('"', "")));
        }
        namespaces.push((namespace.to_owned(), values));
    }
    Ok(namespaces)
}

fn image_name(path: &Path) -> anyhow::Result<String> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow::anyhow!("image filename is not valid UTF-8: {}", path.display()))?;
    let Some((name, dimensions)) = stem.rsplit_once('_') else {
        bail!(
            "image filename must end in _<width>x<height>: {}",
            path.display()
        );
    };
    image_dimensions_string(dimensions, path)?;
    Ok(name.to_owned())
}

fn image_dimensions(path: &Path) -> anyhow::Result<(u16, u16)> {
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow::anyhow!("image filename is not valid UTF-8: {}", path.display()))?;
    let (_, dimensions) = stem.rsplit_once('_').ok_or_else(|| {
        anyhow::anyhow!(
            "image filename must end in _<width>x<height>: {}",
            path.display()
        )
    })?;
    image_dimensions_string(dimensions, path)
}

fn image_dimensions_string(dimensions: &str, path: &Path) -> anyhow::Result<(u16, u16)> {
    let Some((width, height)) = dimensions.split_once('x') else {
        bail!(
            "image filename must end in _<width>x<height>: {}",
            path.display()
        );
    };
    Ok((
        width
            .parse()
            .with_context(|| format!("invalid image width in {}", path.display()))?,
        height
            .parse()
            .with_context(|| format!("invalid image height in {}", path.display()))?,
    ))
}

fn format_array(name: &str, bytes: &[u8]) -> String {
    let mut output = format!("uint8_t {name}[] =\n{{\n");
    for (index, byte) in bytes.iter().enumerate() {
        if index % 16 == 0 {
            output.push_str("    ");
        }
        output.push_str(&format!("{byte:3},"));
        output.push(if index % 16 != 15 { ' ' } else { '\n' });
    }
    if !bytes.len().is_multiple_of(16) {
        output.push('\n');
    }
    output.push_str("};\n");
    output
}

fn resolve(base: &Path, path: PathBuf) -> PathBuf {
    if path.is_relative() {
        base.join(path)
    } else {
        path
    }
}

fn write_output(path: &Path, contents: String) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(path, contents).with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{format_array, parse_legacy_strings};

    #[test]
    fn preserves_legacy_whitespace_and_namespace_order() {
        let parsed = parse_legacy_strings(
            "namespace First {\n item = \"one\" \n}\nnamespace Second {\nnext = \"two\"\n}",
        )
        .unwrap();
        assert_eq!(
            parsed,
            vec![
                (
                    "First".to_owned(),
                    vec![("item".to_owned(), " one".to_owned())]
                ),
                (
                    "Second".to_owned(),
                    vec![("next".to_owned(), " two".to_owned())]
                )
            ]
        );
    }

    #[test]
    fn formats_legacy_c_arrays() {
        assert_eq!(
            format_array("sprite", &[1, 2]),
            "uint8_t sprite[] =\n{\n      1,   2, \n};\n"
        );
    }
}
