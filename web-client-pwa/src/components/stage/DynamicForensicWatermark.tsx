"use client";

import React, { useEffect, useState } from "react";

export interface DynamicForensicWatermarkProps {
  participantName: string;
  userId?: string;
  userPhone?: string;
  enabled?: boolean;
  opacity?: number; // default: 0.12 (subtle & non-intrusive)
}

export const DynamicForensicWatermark: React.FC<DynamicForensicWatermarkProps> = ({
  participantName,
  userId,
  userPhone,
  enabled = true,
  opacity = 0.12,
}) => {
  const [timestamp, setTimestamp] = useState<string>("");
  const [position, setPosition] = useState<{ x: number; y: number }>({ x: 20, y: 30 });

  // Update timestamp every 10 seconds
  useEffect(() => {
    if (!enabled) return;

    const updateTime = () => {
      const now = new Date();
      setTimestamp(
        `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(
          now.getDate()
        ).padStart(2, "0")} ${String(now.getHours()).padStart(2, "0")}:${String(
          now.getMinutes()
        ).padStart(2, "0")}:${String(now.getSeconds()).padStart(2, "0")}`
      );
    };

    updateTime();
    const interval = setInterval(updateTime, 10000);
    return () => clearInterval(interval);
  }, [enabled]);

  // Slowly drift position every 12 seconds to prevent static cropping
  useEffect(() => {
    if (!enabled) return;

    const drift = () => {
      // Random coordinates within 10% - 80% range of the container
      const newX = 10 + Math.floor(Math.random() * 70);
      const newY = 15 + Math.floor(Math.random() * 65);
      setPosition({ x: newX, y: newY });
    };

    const interval = setInterval(drift, 12000);
    return () => clearInterval(interval);
  }, [enabled]);

  if (!enabled || !participantName) return null;

  const displayPhone = userPhone ? ` • ${userPhone}` : "";
  const displayId = userId ? ` [${userId.substring(0, 8)}]` : "";

  return (
    <div
      data-testid="forensic-watermark"
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 z-40 overflow-hidden select-none transition-all duration-1000 ease-in-out"
    >
      <div
        className="absolute transform -rotate-12 whitespace-nowrap text-xs font-mono font-bold tracking-widest text-white/90 drop-shadow-sm select-none"
        style={{
          left: `${position.x}%`,
          top: `${position.y}%`,
          opacity,
          transition: "left 4s ease-in-out, top 4s ease-in-out",
        }}
      >
        <span>
          {participantName}
          {displayId}
          {displayPhone}
          {" — "}
          {timestamp}
        </span>
      </div>
    </div>
  );
};
