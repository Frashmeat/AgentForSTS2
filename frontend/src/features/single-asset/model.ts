export type AssetType = "card" | "card_fullscreen" | "relic" | "power" | "character" | "custom_code";

export type Stage =
  | "input"
  | "confirm_prompt"
  | "generating_image"
  | "pick_image"
  | "agent_running"
  | "approval_pending"
  | "done"
  | "error";

export interface AssetTypeOption {
  value: AssetType;
  label: string;
  desc: string;
  imgHint: string;
}

export interface PresetOption {
  label: string;
  assetType: AssetType;
  assetName: string;
  description: string;
}

export const ASSET_TYPES: AssetTypeOption[] = [
  { value: "card", label: "卡牌", desc: "普通卡牌", imgHint: "横向图，建议 250×190 或更大 → 自动生成 ×2（小图 + 大图）" },
  { value: "card_fullscreen", label: "全画面卡", desc: "全画面卡牌", imgHint: "竖向图，建议 250×350 或更大 → 自动生成 ×2（小图 + 大图）" },
  { value: "relic", label: "遗物", desc: "遗物", imgHint: "方形图，主体居中 → 自动抠图，生成 ×3（图标 94×94 + 描边 + 大图 256×256）" },
  { value: "power", label: "Power", desc: "技能/状态图标", imgHint: "方形图标 → 自动抠图，生成 ×2（64×64 + 256×256）" },
  { value: "character", label: "角色", desc: "角色", imgHint: "人物立绘，方形或竖向 → 自动抠图，生成 ×4（图标 + 选角图 + 锁定版 + 地图标记）" },
  { value: "custom_code", label: "自定义代码", desc: "纯代码逻辑", imgHint: "无图像阶段，直接进入 Code Agent / 服务器文本方案生成" },
];

export const PRESETS: PresetOption[] = [
  {
    label: "BloodLance",
    assetType: "card",
    assetName: "BloodLance",
    description: "攻击牌，费用1，造成7点伤害；如果目标身上有中毒层数，额外造成等于中毒层数的伤害。升级后基础伤害提升到10。",
  },
  {
    label: "攻击卡",
    assetType: "card",
    assetName: "DarkBlade",
    description: "一把暗黑匕首，造成8点伤害，升级后造成12点伤害",
  },
  {
    label: "遗物",
    assetType: "relic",
    assetName: "FangedGrimoire",
    description: "每次造成伤害时，获得2点格挡。稀有度：普通",
  },
  {
    label: "Power",
    assetType: "power",
    assetName: "CorruptionBuff",
    description: "腐化叠层 buff：每叠加1层，回合结束时额外造成1点伤害，最多叠加10层",
  },
  {
    label: "自定义代码",
    assetType: "custom_code",
    assetName: "BattleScriptManager",
    description: "实现一个战斗阶段脚本管理器，负责监听阶段切换、派发事件，并给后续战斗机制提供统一入口。",
  },
];

const ORDER: Stage[] = ["input", "confirm_prompt", "generating_image", "pick_image", "agent_running", "approval_pending", "done"];

export function getStageIndex(stage: Stage) {
  const index = ORDER.indexOf(stage);
  return index === -1 ? ORDER.indexOf("agent_running") : index;
}
