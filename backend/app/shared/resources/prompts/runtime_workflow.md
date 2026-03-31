## workflow_approval_output_default
Approval required before execution

## workflow_approval_cancelled_output
用户取消执行

## workflow_approval_passed_stage
审批通过，开始生成代码...

## workflow_approval_passed_progress
审批通过，Code Agent 开始生成代码...

## workflow_prompt_preview_error
workflow error

## workflow_build_started
开始构建

## workflow_build_finished
构建完成

## workflow_project_init_stage
正在初始化项目 {{ project_name }}...

## workflow_project_init_progress
正在从本地模板初始化项目 {{ project_name }}...

## workflow_project_init_done
项目初始化完成: {{ project_root }}

## workflow_prompt_adapting_stage
正在整理图像提示词...

## workflow_prompt_adapting_progress
正在生成图像提示词...

## workflow_image_generating_stage
正在生成第 {{ image_number }} 张图像...

## workflow_image_generating_progress
正在生成第 {{ image_number }} 张图像…

## workflow_image_postprocess_stage
正在处理图像资产...

## workflow_image_postprocess_progress
正在处理图像资产...

## workflow_image_paths_written
图像资产已写入: {{ image_paths }}

## workflow_agent_running_stage
正在生成代码...

## workflow_agent_running_progress
Code Agent 开始生成代码...

## workflow_custom_code_agent_running_stage
正在生成自定义代码...

## workflow_custom_code_agent_running_progress
Code Agent 开始生成自定义代码...

## workflow_provided_image_missing
图片文件不存在：{{ image_path }}

## workflow_provided_image_reading
读取图片：{{ file_name }}

## workflow_api_key_invalid
API Key 无效（401 Unauthorized）。请在设置中填写正确的 API Key。

## workflow_api_key_forbidden
API Key 无权限（403 Forbidden）。请确认 Key 已开通对应模型的访问权限。

## workflow_network_error
网络连接失败，无法访问图像生成 API。请检查网络或代理设置。
({{ error_type }})

## workflow_timeout_error
请求超时。图像生成 API 响应过慢，请稍后重试。

## batch_approval_summary_default
This group requires approval before execution

## batch_approval_error_default
workflow error

## batch_api_requirements_missing
requirements 不能为空

## batch_start_action_expected
期望 action=start 或 start_with_plan

## batch_confirm_plan_expected
期望 action=confirm_plan

## batch_planning_stage
正在规划 Mod...

## batch_project_init_stage
正在初始化项目 {{ project_name }}...

## batch_project_init_progress
未检测到项目，正在创建 {{ project_name }}...

## batch_project_init_done
项目创建完成: {{ project_root }}

## batch_multi_group_detected
发现 {{ group_count }} 个依赖组，将合并生成代码（更快）

## batch_provided_image_progress
使用用户提供的图片...

## batch_prompt_adapting_stage
正在整理图像提示词...

## batch_prompt_adapting_progress
正在优化图像提示词...

## batch_image_generating_stage
正在生成第 {{ image_number }} 张图像...

## batch_image_generating_progress
正在生成第 {{ image_number }} 张图像...

## batch_image_generating_retry
图像生成失败，重试 {{ retry_number }}/3...

## batch_image_postprocess_stage
正在处理图像资产...

## batch_image_postprocess_progress
正在处理图像资产...

## batch_image_postprocess_done
图像资产处理完成，等待组内其他资产...

## batch_group_image_failure_skip
组内资产 {{ failed_items }} 图片生成失败，跳过代码生成

## batch_agent_running_stage
正在生成代码...

## batch_agent_running_progress
Code Agent 开始生成代码...

## batch_build_running_stage
正在统一编译与部署...

## batch_build_running_progress
所有资产代码生成完毕，开始统一编译...

## batch_build_success_progress
✓ 编译成功，DLL 和 .pck 已部署

## batch_build_failure_progress
⚠ 编译失败，请检查代码错误

## build_project_root_missing
路径不存在：{{ project_root }}

## build_agent_build_start
▶ Code Agent 开始构建...

## build_build_failed
构建失败，请查看上方 Agent 输出

## build_build_succeeded
✓ 构建成功！

## build_deployed_via_local_props
✓ 已通过 local.props 部署到 {{ target_dir }}

## build_copying_to_target
▶ 复制到 {{ target_dir }}...

## build_deploy_finished
✓ 部署完成！

## build_output_missing
⚠ 未找到构建产物（bin/ 和 Mods 均无 .dll/.pck）

## build_game_path_missing
⚠ 未配置 STS2 游戏路径，跳过部署（可在设置中配置）

## build_file_item
  ✓ {{ file_name }}
