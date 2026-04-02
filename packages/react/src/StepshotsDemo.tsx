import React, { useMemo } from "react";

export interface StephotsDemoProps {
  demoId: string;
  baseUrl?: string;
  autoplay?: boolean;
  theme?: "light" | "dark";
  start?: number;
  hideControls?: boolean;
  width?: string | number;
  aspectRatio?: string;
  style?: React.CSSProperties;
  className?: string;
}

export function StepshotsDemo({
  demoId,
  baseUrl = "https://app.stepshots.com",
  autoplay,
  theme,
  start,
  hideControls,
  width = "100%",
  aspectRatio = "16/9",
  style,
  className,
}: StephotsDemoProps) {
  const iframeUrl = useMemo(() => {
    const params = new URLSearchParams();
    if (autoplay) params.set("autoplay", "true");
    if (theme) params.set("theme", theme);
    if (start !== undefined && start > 0) params.set("start", String(start));
    if (hideControls) params.set("hide_controls", "true");
    const qs = params.toString();
    return `${baseUrl}/embed/${encodeURIComponent(demoId)}${qs ? `?${qs}` : ""}`;
  }, [demoId, baseUrl, autoplay, theme, start, hideControls]);

  return (
    <div
      className={className}
      style={{
        position: "relative",
        width,
        aspectRatio,
        overflow: "hidden",
        borderRadius: "12px",
        background: "#1a1a2e",
        ...style,
      }}
    >
      <iframe
        src={iframeUrl}
        style={{
          position: "absolute",
          top: 0,
          left: 0,
          width: "100%",
          height: "100%",
          border: "none",
        }}
        allowFullScreen
        loading="lazy"
        title="Stepshots Demo"
      />
    </div>
  );
}
