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
    shades: Option<u8>,
    #[serde(default, rename = "set")]
    sets: Vec<Set>,
    strings: Option<Strings>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Set {
    output: PathBuf,
    images: Vec<PathBuf>,
    shades: Option<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Strings {
    output: PathBuf,
    font: PathBuf,
    input: PathBuf,
    glyph_width: u16,
    glyph_height: u16,
    shades: Option<u8>,
    order: Vec<String>,
}

pub(crate) struct ReferencedImage {
    pub path: PathBuf,
    pub archive_relative: PathBuf,
}

fn resolve_shades(explicit: Option<u8>, top_level: Option<u8>, name: &str) -> anyhow::Result<u8> {
    let shades = explicit.or(top_level).unwrap_or(4);
    if !(2..=4).contains(&shades) {
        bail!("{name} shades must be 2, 3, or 4, got {shades}");
    }
    Ok(shades)
}

fn load_config(config_path: &Path) -> anyhow::Result<Config> {
    let contents = std::fs::read_to_string(config_path)
        .with_context(|| format!("reading sprite source config {}", config_path.display()))?;
    toml::from_str(&contents)
        .with_context(|| format!("parsing sprite source config {}", config_path.display()))
}

pub(crate) fn referenced_images(config_path: &Path) -> anyhow::Result<Vec<ReferencedImage>> {
    let config = load_config(config_path)?;
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen = HashSet::new();
    let mut images = Vec::new();

    for set in config.sets {
        for raw in set.images {
            let path = resolve(base, raw.clone());
            if seen.insert(path.clone()) {
                images.push(ReferencedImage {
                    path,
                    archive_relative: archive_relative(&raw),
                });
            }
        }
    }
    if let Some(strings) = config.strings {
        let path = resolve(base, strings.font.clone());
        if seen.insert(path.clone()) {
            images.push(ReferencedImage {
                path,
                archive_relative: archive_relative(&strings.font),
            });
        }
    }
    Ok(images)
}

fn archive_relative(path: &Path) -> PathBuf {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(value) => relative.push(value),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !relative.pop() {
                    relative.push("_parent");
                }
            }
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                relative.push("_absolute");
            }
        }
    }
    relative
}

/// Emit legacy `Sprites.txt` C arrays from PNG/font sources without Python.
pub fn emit(config_path: &Path) -> anyhow::Result<()> {
    let config = load_config(config_path)?;
    let base = config_path.parent().unwrap_or_else(|| Path::new("."));

    for set in config.sets {
        let shades = resolve_shades(
            set.shades,
            config.shades,
            &format!("set output {}", set.output.display()),
        )?;
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
                    shades,
                    spacing: 0,
                },
            )?;
            output.push_str(&format_array(&name, &sheet.bytes));
        }
        write_output(&resolve(base, set.output), output)?;
    }

    if let Some(strings) = config.strings {
        emit_strings(base, strings, config.shades)?;
    }
    Ok(())
}

fn emit_strings(base: &Path, config: Strings, top_level_shades: Option<u8>) -> anyhow::Result<()> {
    let shades = resolve_shades(config.shades, top_level_shades, "strings")?;
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
            shades,
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
    use super::{Config, format_array, parse_legacy_strings, resolve_shades};
    use crate::fxpack::sprite::{self, Params};

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

    #[test]
    fn absent_shades_resolves_to_byte_identical_legacy_default() {
        let config: Config =
            toml::from_str("[[set]]\noutput = \"sprites.h\"\nimages = []\n").unwrap();
        let shades =
            resolve_shades(config.sets[0].shades, config.shades, "set output sprites.h").unwrap();
        assert_eq!(shades, 4);

        let pixels = [0, 0, 0, 255, 255, 255, 255, 255];
        let default = sprite::encode_rgba_legacy(
            &pixels,
            2,
            1,
            Params {
                width: 2,
                height: 1,
                shades,
                spacing: 0,
            },
        )
        .unwrap();
        assert_eq!(default.bytes, vec![0, 1, 0, 1, 0, 1]);
    }

    #[test]
    fn top_level_two_shades_uses_one_plane_and_128_threshold() {
        let pixels: Vec<u8> = [127, 128, 200, 100, 255, 0, 128, 127]
            .into_iter()
            .flat_map(|red| [red, 0, 0, 255])
            .collect();
        let sheet = sprite::encode_rgba_legacy(
            &pixels,
            1,
            8,
            Params {
                width: 1,
                height: 8,
                shades: resolve_shades(None, Some(2), "set output sprites.h").unwrap(),
                spacing: 0,
            },
        )
        .unwrap();
        assert_eq!(sheet.bytes.len(), 1);
        assert_eq!(sheet.bytes, vec![0b0101_0110]);

        let mut masked_pixels = pixels;
        masked_pixels[2 * 4 + 3] = 0;
        let masked = sprite::encode_rgba_legacy(
            &masked_pixels,
            1,
            8,
            Params {
                width: 1,
                height: 8,
                shades: 2,
                spacing: 0,
            },
        )
        .unwrap();
        assert_eq!(masked.bytes.len(), 2);
        assert_eq!(masked.bytes, vec![0b0101_0110, 0b1111_1011]);
    }

    #[test]
    fn per_set_overrides_top_level_and_strings_fallback_or_override() {
        let config: Config = toml::from_str(
            "shades = 2\n\n[[set]]\noutput = \"sprites.h\"\nimages = []\nshades = 4\n\n[strings]\noutput = \"strings.h\"\nfont = \"font.png\"\ninput = \"strings.txt\"\nglyph_width = 8\nglyph_height = 8\norder = []\n",
        )
        .unwrap();
        assert_eq!(
            resolve_shades(config.sets[0].shades, config.shades, "set output sprites.h").unwrap(),
            4
        );
        let strings = config.strings.unwrap();
        assert_eq!(
            resolve_shades(strings.shades, config.shades, "strings").unwrap(),
            2
        );

        let explicit: Config = toml::from_str(
            "shades = 2\n[strings]\noutput = \"strings.h\"\nfont = \"font.png\"\ninput = \"strings.txt\"\nglyph_width = 8\nglyph_height = 8\nshades = 3\norder = []\n",
        )
        .unwrap();
        let strings = explicit.strings.unwrap();
        assert_eq!(
            resolve_shades(strings.shades, explicit.shades, "strings").unwrap(),
            3
        );
    }

    #[test]
    fn invalid_shades_name_set_output_or_strings() {
        let set_error = resolve_shades(Some(1), None, "set output sprites.h").unwrap_err();
        assert!(set_error.to_string().contains("set output sprites.h"));
        assert!(set_error.to_string().contains("got 1"));

        let strings_error = resolve_shades(Some(5), None, "strings").unwrap_err();
        assert!(strings_error.to_string().contains("strings"));
        assert!(strings_error.to_string().contains("got 5"));
    }

    #[test]
    fn unknown_keys_remain_rejected_on_all_three_structs() {
        let top_level = toml::from_str::<Config>("unknown = 2\n")
            .err()
            .expect("unknown top-level key must fail");
        assert!(top_level.to_string().contains("unknown"));

        let set =
            toml::from_str::<Config>("[[set]]\noutput = \"sprites.h\"\nimages = []\nunknown = 2\n")
                .err()
                .expect("unknown set key must fail");
        assert!(set.to_string().contains("unknown"));

        let strings = toml::from_str::<Config>(
            "[strings]\noutput = \"strings.h\"\nfont = \"font.png\"\ninput = \"strings.txt\"\nglyph_width = 8\nglyph_height = 8\norder = []\nunknown = 2\n",
        )
        .err()
        .expect("unknown strings key must fail");
        assert!(strings.to_string().contains("unknown"));
    }
}
