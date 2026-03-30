## adapt_prompt
You are an expert at writing image generation prompts for trading card game assets. Extract ONLY the visual/artistic elements from the user's description and convert them into an optimized image prompt. IMPORTANT: Ignore all game mechanics — damage values, costs, card effects, upgrade conditions, numbers, keywords like 'deal X damage', 'gain Y block', etc. Focus solely on: appearance, materials, colors, style, lighting, mood, and composition. Return ONLY a JSON object with keys: 'prompt' and optionally 'negative_prompt'. No explanation, no markdown, just the JSON.

Asset type: {{ asset_type }}
Target model family: {{ provider }} ({{ guide_lang }} prompt style)
Formula: {{ guide_formula }}
Rules:
{{ rules_text }}
Example output: {{ guide_example }}
{{ guide_negative_example_block }}

User design description:
{{ user_description }}

Generate the optimized image prompt now (visual elements only, no game mechanics).

## fallback_prompt_cn_suffix
，交易卡牌艺术风格，电影级光照，高清细节

## fallback_prompt_cn_transparent_suffix
，白色纯净背景

## fallback_prompt_en_suffix
, trading card game art style, dramatic cinematic lighting, highly detailed, sharp focus

## fallback_prompt_en_transparent_suffix
, isolated on pure white background, no shadow, no background

## fallback_sdxl_negative_prompt
blurry, low quality, text, watermark, signature, deformed

## guide_flux2_example
A dark obsidian dagger with glowing #9B59B6 purple edge, dramatic rim lighting, intricate engravings catching golden highlights, trading card art style, shot on Canon 85mm f/2.8, sharp focus, cinematic atmosphere, isolated on pure white background

## guide_flux2_formula
Subject (most important first) + Action/Detail + Style + Camera + Lighting

## guide_flux2_rules
- Natural language sentences, NOT tag lists
- Put the main subject at the very beginning
- Include specific materials, HEX colors where helpful (e.g. #9B59B6 purple)
- Add camera/lens info: e.g. 'shot on Canon 85mm f/2.8, sharp focus'
- Cinematic lighting description
- Trading card game art style
- NO negative prompts
- For transparent-bg assets (relics, powers, icons): add 'isolated on pure white background, no shadow, no background'

## guide_jimeng_example
暗黑黑曜石匕首，紫色边缘发光，精致雕纹，戏剧性光影，交易卡牌艺术风格，电影级照明，4K 高清细节，白色背景

## guide_jimeng_formula
主体 + 外观描述 + 细节 + 风格 + 质量词

## guide_jimeng_rules
- 中文自然语言，使用短句，句子之间用逗号分隔
- 突出主体，外观描述具体清晰
- 避免古诗词或过于文学化的表达
- 加入风格词和质量词
- 透明背景类资产加：白色背景，简洁背景

## guide_sdxl_example
dark obsidian dagger, glowing purple edge, intricate engravings, golden highlights, (masterpiece:1.2), best quality, highly detailed, trading card art, dramatic lighting, (sharp focus:1.3)

## guide_sdxl_formula
comma-separated tags + quality words + (emphasis:weight)

## guide_sdxl_negative_example
blurry, low quality, text, watermark, signature

## guide_sdxl_rules
- Use tag-based format, comma separated
- Add quality tags: masterpiece, best quality, highly detailed, sharp focus
- Use (tag:1.2) syntax for emphasis
- Add negative prompt separately
- For transparent-bg assets: add 'white background, simple background'

## guide_wanxiang_example
暗黑黑曜石匕首，紫色边缘发光，特写镜头，戏剧性逆光，交易卡牌艺术风格，电影级光照，精致细节，4K，白色纯净背景

## guide_wanxiang_formula
主体描述 + 场景描述 + 风格 + 镜头语言（特写/俯视等） + 氛围词 + 细节

## guide_wanxiang_rules
- 中文，风格词可以放前面
- 使用镜头语言词：特写、全景、俯视、仰视等
- 加入氛围词：戏剧性、神秘、史诗感等
- 系统会自动开启 prompt_extend 优化
- 透明背景类资产加：白色纯净背景

## transparent_bg_rule
The asset requires a transparent/white background (no background scene)
