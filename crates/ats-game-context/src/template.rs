use ats_kernel::{ProjectTemplateBundle, ProjectTemplateFile, Sha256Digest};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::LoadedGamePack;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ProjectTemplateError {
    #[error("project template is not registered for this Pack")]
    UnknownTemplate,
    #[error("built-in project template is invalid")]
    InvalidTemplate,
}

pub fn built_in_project_template(
    pack: &LoadedGamePack,
    template_id: &str,
) -> Result<ProjectTemplateBundle, ProjectTemplateError> {
    if pack.id().as_str() != "sts2" || template_id != "sts2.default" {
        return Err(ProjectTemplateError::UnknownTemplate);
    }
    macro_rules! file {
        ($path:literal) => {{
            const BYTES: &[u8] =
                include_bytes!(concat!("../../../game_packs/sts2/template/", $path));
            ProjectTemplateFile {
                relative_path: $path.into(),
                sha256: Sha256Digest::parse(format!("{:x}", Sha256::digest(BYTES)))
                    .expect("SHA-256 formatter is valid"),
                bytes: BYTES.to_vec(),
            }
        }};
    }
    let bundle = ProjectTemplateBundle {
        game_pack_id: pack.id().clone(),
        template_id: template_id.into(),
        placeholder: "ModTemplate".into(),
        files: vec![
            file!(".gitattributes"),
            file!(".gitignore"),
            file!("export_presets.cfg"),
            file!("local.props.example"),
            file!("MainFile.cs"),
            file!("ModTemplate.csproj"),
            file!("ModTemplate.json"),
            file!("ModTemplate.sln"),
            file!("ModTemplate.sln.DotSettings"),
            file!("nuget.config"),
            file!("project.godot"),
            file!("Extensions/StringExtensions.cs"),
            file!("packages/godot.net.sdk/4.5.1/.nupkg.metadata"),
            file!("packages/godot.net.sdk/4.5.1/.signature.p7s"),
            file!("packages/godot.net.sdk/4.5.1/godot.net.sdk.4.5.1.nupkg"),
            file!("packages/godot.net.sdk/4.5.1/godot.net.sdk.4.5.1.nupkg.sha512"),
            file!("packages/godot.net.sdk/4.5.1/godot.net.sdk.nuspec"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/Android.props"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/iOSNativeAOT.props"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/iOSNativeAOT.targets"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/Sdk.props"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/Sdk.targets"),
            file!("packages/godot.net.sdk/4.5.1/Sdk/SdkPackageVersions.props"),
        ],
    };
    bundle
        .validate()
        .map_err(|_| ProjectTemplateError::InvalidTemplate)?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use crate::GamePackLoader;

    use super::*;

    #[test]
    fn sts2_template_is_pack_owned_and_complete() {
        let pack = GamePackLoader::load_built_in_sts2().unwrap();
        let template = built_in_project_template(&pack, "sts2.default").unwrap();
        assert!(template.files.len() > 20);
        assert!(
            template
                .files
                .iter()
                .any(|file| file.relative_path == "ModTemplate.csproj")
        );
    }
}
