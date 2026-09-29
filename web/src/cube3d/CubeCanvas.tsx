import { useEffect, useRef } from "react";
import type { CubeController } from "../engine/controller";
import { CubeStage } from "./CubeStage";

export function CubeCanvas({ controller }: { controller: CubeController }) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const stage = new CubeStage(el);
    controller.attachStage(stage);
    return () => {
      controller.attachStage(null);
      stage.dispose();
    };
  }, [controller]);

  return <div ref={ref} className="absolute inset-0 cursor-grab active:cursor-grabbing" />;
}
