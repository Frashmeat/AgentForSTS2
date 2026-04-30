import { useState } from "react";
import { CheckCircle } from "lucide-react";
import { cn } from "../lib/utils";

interface Props {
  images: string[]; // base64 PNG strings
  onSelect: (index: number) => void;
  disabled?: boolean;
}

export function ImagePicker({ images, onSelect, disabled }: Props) {
  const [selected, setSelected] = useState<number | null>(null);

  function handleClick(i: number) {
    if (disabled) return;
    setSelected(i);
    onSelect(i);
  }

  return (
    <div className="space-y-3">
      <p className="text-spire-muted text-sm">选择一张作为资产图片：</p>
      <div className="grid grid-cols-3 gap-4">
        {images.map((b64, i) => (
          <button
            key={i}
            onClick={() => handleClick(i)}
            disabled={disabled}
            className={cn(
              "relative rounded-lg border-2 overflow-hidden transition-all",
              selected === i
                ? "border-spire-accent shadow-lg shadow-spire-accent/30"
                : "border-spire-border hover:border-spire-purple",
              disabled && "opacity-50 cursor-not-allowed",
            )}
          >
            <img
              src={`data:image/png;base64,${b64}`}
              alt={`生成图 ${i + 1}`}
              className="w-full object-contain bg-spire-surface"
            />
            {selected === i && (
              <div className="absolute top-2 right-2 text-spire-accent">
                <CheckCircle size={20} />
              </div>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}
