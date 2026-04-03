## config_image_test_prompt
a glowing icon

## project_utils_local_props_template
<Project>
  <PropertyGroup>
    <STS2GamePath>{{ sts2_path }}</STS2GamePath>
    <GodotExePath>{{ godot_path }}</GodotExePath>
  </PropertyGroup>
</Project>

## project_utils_sts2_found_via_registry
通过 Steam 注册表找到 STS2: {{ path }}

## project_utils_sts2_found_in_common_paths
在常见路径找到 STS2: {{ path }}

## project_utils_sts2_not_found
未能自动找到 STS2，请手动填写路径

## project_utils_godot_found
找到 Godot: {{ path }}

## project_utils_godot_found_in_path
在 PATH 中找到 Godot: {{ path }}

## project_utils_godot_not_found
未能自动找到 Godot 4.5.1 Mono，请手动填写路径

## project_utils_template_missing
Mod 模板目录不存在: {{ template_path }}
请将模板项目放到 {{ default_template_path }}，
或在 config.json 中设置 mod_template_path。
