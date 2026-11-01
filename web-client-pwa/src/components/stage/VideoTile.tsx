"use client";

import React, { useRef } from "react";
import {
  useParticipant,
  useParticipantTrack,
  useParticipantQuality,
} from "../../store/use_participants_store";
import {
  useIsParticipantSpeaking,
  useParticipantAudioLevel,
} from "../../store/use_telemetry_store";
import { useMediaTrackAttachment } from "./useMediaTrackAttachment";
import { useAdaptiveVideoViewport } from "./useAdaptiveVideoViewport";
import { AdaptiveViewportManager } from "../../application/adaptive_viewport_manager";
import { MicOff, Wifi, WifiOff, User } from "lucide-react";

export interface VideoTileProps {
  participantId: string;
  isLocal?: boolean;
  viewportManager?: AdaptiveViewportManager;
}

export const VideoTile: React.FC<VideoTileProps> = ({
  participantId,
  isLocal = false,
  viewportManager,
}) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const videoRef = useRef<HTMLVideoElement>(null);

  // Atomic selectors for zero re-render lag
  const participant = useParticipant(participantId);
  const videoTrack = useParticipantTrack(participantId, "camera");
  const screenTrack = useParticipantTrack(participantId, "screen");
  const isSpeaking = useIsParticipantSpeaking(participantId);
  const audioLevel = useParticipantAudioLevel(participantId);
  const quality = useParticipantQuality(participantId);

  const activeTrack = screenTrack || videoTrack;
  const isCameraMuted = participant?.isCameraMuted ?? true;
  const isMicMuted = participant?.isMicMuted ?? false;

  // Media track attachment
  useMediaTrackAttachment(videoRef, activeTrack?.mediaStreamTrack);

  // Adaptive Stream observation (for remote video tracks)
  useAdaptiveVideoViewport(
    containerRef,
    !isLocal ? activeTrack?.id : undefined,
    viewportManager
  );

  const name = participant?.name || (isLocal ? "You" : "Participant");

  return (
    <div
      ref={containerRef}
      data-testid={`video-tile-${participantId}`}
      className={`relative w-full h-full rounded-xl overflow-hidden bg-slate-900 border transition-all duration-200 flex items-center justify-center select-none ${
        isSpeaking ? "border-emerald-500 ring-2 ring-emerald-500/50" : "border-slate-800"
      }`}
    >
      {/* Video Element */}
      <video
        ref={videoRef}
        playsInline
        autoPlay
        muted={isLocal}
        className={`w-full h-full object-cover ${
          isCameraMuted && !screenTrack ? "hidden" : "block"
        } ${isLocal && !screenTrack ? "-scale-x-100" : ""}`}
      />

      {/* Camera Muted / Fallback Avatar */}
      {isCameraMuted && !screenTrack && (
        <div className="flex flex-col items-center justify-center text-slate-400">
          <div className="w-16 h-16 sm:w-20 sm:h-20 rounded-full bg-slate-800 flex items-center justify-center border border-slate-700 shadow-inner">
            <User className="w-8 h-8 sm:w-10 sm:h-10 text-slate-300" />
          </div>
          <span className="mt-2 text-sm font-medium text-slate-300">{name}</span>
        </div>
      )}

      {/* Top Indicators: Quality & Speaking level */}
      <div className="absolute top-3 right-3 flex items-center space-x-1.5 bg-slate-900/70 backdrop-blur-md px-2 py-1 rounded-md text-xs text-slate-300 border border-slate-700/50">
        {quality === "lost" ? (
          <WifiOff className="w-3.5 h-3.5 text-red-500" aria-label="Network lost" />
        ) : (
          <Wifi
            className={`w-3.5 h-3.5 ${
              quality === "poor"
                ? "text-amber-500"
                : quality === "excellent"
                ? "text-emerald-400"
                : "text-blue-400"
            }`}
            aria-label={`Network: ${quality}`}
          />
        )}
      </div>

      {/* Bottom Information Bar */}
      <div className="absolute bottom-3 left-3 right-3 flex items-center justify-between pointer-events-none">
        <div className="flex items-center space-x-2 bg-slate-900/80 backdrop-blur-md px-2.5 py-1 rounded-md text-xs text-white border border-slate-700/50 max-w-[80%] truncate">
          <span className="truncate font-medium">{name} {isLocal && "(You)"}</span>
          {isMicMuted && (
            <MicOff className="w-3.5 h-3.5 text-red-400 shrink-0" aria-label="Microphone muted" />
          )}
        </div>

        {/* Audio Level Meter (Active when not muted) */}
        {!isMicMuted && audioLevel > 0.05 && (
          <div
            className="w-2.5 h-2.5 rounded-full bg-emerald-500 animate-pulse shadow-lg shadow-emerald-500/50"
            aria-label="Speaking"
          />
        )}
      </div>
    </div>
  );
};
