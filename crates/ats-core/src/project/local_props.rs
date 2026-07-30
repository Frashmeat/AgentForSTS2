use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use quick_xml::events::{BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use thiserror::Error;

use crate::fs_atomic::write_atomic_sync;
use crate::game_pack::BuildRecipe;

#[derive(Debug, Clone)]
pub struct LocalBuildInputs {
    pub values: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LocalPropsSync {
    pub path: PathBuf,
    pub created: bool,
}

#[derive(Debug, Error)]
pub enum LocalPropsError {
    #[error("build recipe requires local input `{0}`")]
    MissingInput(String),
    #[error("game pack does not declare a build recipe")]
    MissingBuildRecipe,
    #[error("local.props has no PropertyGroup")]
    MissingPropertyGroup,
    #[error("local.props I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("local.props XML failed: {0}")]
    Xml(#[from] quick_xml::Error),
}

pub fn sync_local_props(
    project_root: &Path,
    recipe: Option<&BuildRecipe>,
    inputs: &LocalBuildInputs,
) -> Result<LocalPropsSync, LocalPropsError> {
    let recipe = recipe.ok_or(LocalPropsError::MissingBuildRecipe)?;
    let target = project_root.join("local.props");
    let created = !target.exists();
    let source = if created {
        project_root.join("local.props.example")
    } else {
        target.clone()
    };
    let xml = fs::read_to_string(source)?;
    let properties = recipe
        .local_properties
        .iter()
        .map(|mapping| {
            let value = inputs
                .values
                .get(&mapping.input_key)
                .ok_or_else(|| LocalPropsError::MissingInput(mapping.input_key.clone()))?;
            Ok((
                mapping.property.clone(),
                value.to_string_lossy().into_owned(),
            ))
        })
        .collect::<Result<Vec<_>, LocalPropsError>>()?;
    let output = replace_managed_properties(&xml, &properties)?;
    write_atomic_sync(&target, &output)?;
    Ok(LocalPropsSync {
        path: target,
        created,
    })
}

fn replace_managed_properties(
    xml: &str,
    properties: &[(String, String)],
) -> Result<Vec<u8>, LocalPropsError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::with_capacity(xml.len() + 128));
    let mut replacement: Option<(usize, usize)> = None;
    let mut seen = vec![false; properties.len()];
    let mut found_property_group = false;
    let mut inserted_missing = false;

    loop {
        let event = reader.read_event()?;
        if let Some((property_index, depth)) = replacement.as_mut() {
            match event {
                Event::Start(_) => *depth += 1,
                Event::End(end) => {
                    *depth -= 1;
                    if *depth == 0 {
                        let value = &properties[*property_index].1;
                        writer.write_event(Event::Text(BytesText::new(value)))?;
                        writer.write_event(Event::End(end.into_owned()))?;
                        replacement = None;
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            continue;
        }

        match event {
            Event::Start(start) => {
                if start.name().as_ref() == b"PropertyGroup" {
                    found_property_group = true;
                }
                let property = properties
                    .iter()
                    .position(|(name, _)| start.name().as_ref() == name.as_bytes());
                writer.write_event(Event::Start(start.into_owned()))?;
                if let Some(index) = property {
                    seen[index] = true;
                    replacement = Some((index, 1));
                }
            }
            Event::End(end) if end.name().as_ref() == b"PropertyGroup" && !inserted_missing => {
                for (index, (name, value)) in properties.iter().enumerate() {
                    if seen[index] {
                        continue;
                    }
                    writer.write_event(Event::Start(BytesStart::new(name)))?;
                    writer.write_event(Event::Text(BytesText::new(value)))?;
                    writer.write_event(Event::End(BytesEnd::new(name)))?;
                }
                inserted_missing = true;
                writer.write_event(Event::End(end.into_owned()))?;
            }
            Event::Empty(empty) if empty.name().as_ref() == b"PropertyGroup" => {
                found_property_group = true;
                writer.write_event(Event::Start(empty.into_owned()))?;
                for (name, value) in properties {
                    writer.write_event(Event::Start(BytesStart::new(name)))?;
                    writer.write_event(Event::Text(BytesText::new(value)))?;
                    writer.write_event(Event::End(BytesEnd::new(name)))?;
                }
                inserted_missing = true;
                writer.write_event(Event::End(BytesEnd::new("PropertyGroup")))?;
            }
            Event::Empty(empty) => {
                let property = properties
                    .iter()
                    .position(|(name, _)| empty.name().as_ref() == name.as_bytes());
                if let Some(index) = property {
                    seen[index] = true;
                    let (name, value) = &properties[index];
                    writer.write_event(Event::Start(empty.into_owned()))?;
                    writer.write_event(Event::Text(BytesText::new(value)))?;
                    writer.write_event(Event::End(BytesEnd::new(name)))?;
                } else {
                    writer.write_event(Event::Empty(empty.into_owned()))?;
                }
            }
            Event::Eof => break,
            other => writer.write_event(other.into_owned())?,
        }
    }
    if !found_property_group {
        return Err(LocalPropsError::MissingPropertyGroup);
    }
    Ok(writer.into_inner())
}
