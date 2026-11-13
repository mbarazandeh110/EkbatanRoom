"use client";

import React, { useEffect, useState, useCallback } from "react";
import {
  Mic,
  MicOff,
  Video,
  VideoOff,
  Monitor,
  MonitorOff,
  Grid,
  Maximize2,
  PhoneOff,
  Wifi,
  Sparkles,
  Wand2,
  Disc,
  Hand,
} from "lucide-react";
import { useRoomStore } from "../../store/use_room_store";
import { useParticipantsStore } from "../../store/use_participants_store";
import { RoomSessionController } from "../../application/room_session_controller";

export interface ControlBarProps {
  controller: RoomSessionController;
  onLeave?: () => void;
  onToggleScreenShare?: () => void;
  isScreenSharing?: boolean;
  onToggleNoiseSuppression?: () => void;
  isNoiseSuppressionEnabled?: boolean;
  onToggleVirtualBackground?: () => void;
  isVirtualBackgroundEnabled?: boolean;
  onToggleRecording?: () => void;
  isRecording?: boolean;
  onRaiseHand?: () => void;
  isHandRaised?: boolean;
}

export const ControlBar: React.FC<ControlBarProps> = ({
  controller,
  onLeave,
  onToggleScreenShare,
  isScreenSharing = false,
  onToggleNoiseSuppression,
  isNoiseSuppressionEnabled = false,
  onToggleVirtualBackground,
  isVirtualBackgroundEnabled = false,
  onToggleRecording,
  isRecording = false,
  onRaiseHand,
  isHandRaised = false,
}) => {
  const localParticipantId = useRoomStore((state) => state.localParticipantId);
  const layoutMode = useRoomStore((state) => state.layoutMode);
  const setLayoutMode = useRoomStore((state) => state.setLayoutMode);

  const localParticipant = useParticipantsStore(
    (state) => (localParticipantId ? state.participants[localParticipantId] : undefined)
  );

  const [announcement, setAnnouncement] = useState<string>("");

  const isMicMuted = localParticipant?.isMicMuted ?? false;
  const isCameraMuted = localParticipant?.isCameraMuted ?? false;

  const toggleMic = useCallback(async () => {
    try {
      const next = await controller.toggleTrackMute("microphone");
      setAnnouncement(next ? "Microphone muted" : "Microphone unmuted");
    } catch {}
  }, [controller]);

  const toggleCamera = useCallback(async () => {
    try {
      const next = await controller.toggleTrackMute("camera");
      setAnnouncement(next ? "Camera turned off" : "Camera turned on");
    } catch {}
  }, [controller]);

  const toggleLayout = () => {
    const nextMode = layoutMode === "grid" ? "speaker" : "grid";
    setLayoutMode(nextMode);
    setAnnouncement(`Switched to ${nextMode} layout`);
  };

  // Keyboard shortcuts (m for mic, v for video)
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      ) {
        return;
      }

      if (e.key === "m" || e.key === "M") {
        e.preventDefault();
        toggleMic();
      } else if (e.key === "v" || e.key === "V") {
        e.preventDefault();
        toggleCamera();
      } else if (e.key === "h" || e.key === "H") {
        if (onRaiseHand) {
          e.preventDefault();
          onRaiseHand();
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [toggleMic, toggleCamera, onRaiseHand]);

  return (
    <footer
      data-testid="control-bar"
      role="region"
      aria-label="Meeting controls"
      className="h-16 sm:h-20 bg-slate-900/90 backdrop-blur-lg border-t border-slate-800 px-4 flex items-center justify-between shrink-0 select-none"
    >
      {/* ARIA Live Region for screen readers */}
      <div aria-live="polite" aria-atomic="true" className="sr-only">
        {announcement}
      </div>

      {/* Left info: Room name or status */}
      <div className="hidden sm:flex items-center space-x-2 text-slate-400 text-sm">
        <Wifi className="w-4 h-4 text-emerald-400" />
        <span className="font-medium text-slate-300">Connected</span>
      </div>

      {/* Center Controls */}
      <div className="flex items-center space-x-2 sm:space-x-3 mx-auto sm:mx-0 flex-wrap justify-center">
        {/* Microphone Toggle */}
        <button
          type="button"
          onClick={toggleMic}
          aria-label={isMicMuted ? "Unmute microphone (m)" : "Mute microphone (m)"}
          aria-pressed={!isMicMuted}
          className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
            isMicMuted
              ? "bg-red-500/20 text-red-400 border border-red-500/30 hover:bg-red-500/30"
              : "bg-slate-800 text-white hover:bg-slate-700 border border-slate-700"
          }`}
        >
          {isMicMuted ? <MicOff className="w-5 h-5" /> : <Mic className="w-5 h-5" />}
        </button>

        {/* Camera Toggle */}
        <button
          type="button"
          onClick={toggleCamera}
          aria-label={isCameraMuted ? "Turn on camera (v)" : "Turn off camera (v)"}
          aria-pressed={!isCameraMuted}
          className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
            isCameraMuted
              ? "bg-red-500/20 text-red-400 border border-red-500/30 hover:bg-red-500/30"
              : "bg-slate-800 text-white hover:bg-slate-700 border border-slate-700"
          }`}
        >
          {isCameraMuted ? (
            <VideoOff className="w-5 h-5" />
          ) : (
            <Video className="w-5 h-5" />
          )}
        </button>

        {/* Screen Share Toggle */}
        {onToggleScreenShare && (
          <button
            type="button"
            onClick={onToggleScreenShare}
            aria-label={isScreenSharing ? "Stop sharing screen" : "Share screen"}
            aria-pressed={isScreenSharing}
            className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
              isScreenSharing
                ? "bg-blue-600 text-white shadow-lg shadow-blue-500/30"
                : "bg-slate-800 text-white hover:bg-slate-700 border border-slate-700"
            }`}
          >
            {isScreenSharing ? (
              <MonitorOff className="w-5 h-5" />
            ) : (
              <Monitor className="w-5 h-5" />
            )}
          </button>
        )}

        {/* Noise Suppression Toggle */}
        {onToggleNoiseSuppression && (
          <button
            type="button"
            onClick={onToggleNoiseSuppression}
            aria-label={isNoiseSuppressionEnabled ? "Disable noise suppression" : "Enable noise suppression"}
            aria-pressed={isNoiseSuppressionEnabled}
            className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
              isNoiseSuppressionEnabled
                ? "bg-teal-600 text-white shadow-lg shadow-teal-500/30"
                : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
            }`}
          >
            <Sparkles className="w-5 h-5" />
          </button>
        )}

        {/* Virtual Background Toggle */}
        {onToggleVirtualBackground && (
          <button
            type="button"
            onClick={onToggleVirtualBackground}
            aria-label={isVirtualBackgroundEnabled ? "Disable virtual background blur" : "Enable virtual background blur"}
            aria-pressed={isVirtualBackgroundEnabled}
            className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
              isVirtualBackgroundEnabled
                ? "bg-purple-600 text-white shadow-lg shadow-purple-500/30"
                : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
            }`}
          >
            <Wand2 className="w-5 h-5" />
          </button>
        )}

        {/* Raise Hand Toggle */}
        {onRaiseHand && (
          <button
            type="button"
            onClick={onRaiseHand}
            aria-label={isHandRaised ? "Lower hand (h)" : "Raise hand (h)"}
            aria-pressed={isHandRaised}
            className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
              isHandRaised
                ? "bg-amber-600 text-white shadow-lg shadow-amber-500/30 animate-pulse"
                : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
            }`}
          >
            <Hand className="w-5 h-5" />
          </button>
        )}

        {/* Recording Toggle */}
        {onToggleRecording && (
          <button
            type="button"
            onClick={onToggleRecording}
            aria-label={isRecording ? "Stop recording" : "Start local recording"}
            aria-pressed={isRecording}
            className={`p-3 rounded-full transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-red-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900 ${
              isRecording
                ? "bg-red-600 text-white shadow-lg shadow-red-500/30 animate-pulse"
                : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
            }`}
          >
            <Disc className="w-5 h-5" />
          </button>
        )}

        {/* Layout Switcher */}
        <button
          type="button"
          onClick={toggleLayout}
          aria-label={`Switch to ${layoutMode === "grid" ? "speaker" : "grid"} layout`}
          className="p-3 rounded-full bg-slate-800 text-white hover:bg-slate-700 border border-slate-700 transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-emerald-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900"
        >
          {layoutMode === "grid" ? (
            <Maximize2 className="w-5 h-5" />
          ) : (
            <Grid className="w-5 h-5" />
          )}
        </button>

        {/* Leave Room Button */}
        {onLeave && (
          <button
            type="button"
            onClick={onLeave}
            aria-label="Leave call"
            className="p-3 rounded-full bg-red-600 hover:bg-red-700 text-white shadow-lg shadow-red-600/30 transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-red-500 focus-visible:ring-offset-2 focus-visible:ring-offset-slate-900"
          >
            <PhoneOff className="w-5 h-5" />
          </button>
        )}
      </div>

      {/* Right placeholder / space for symmetry */}
      <div className="hidden sm:block w-24" />
    </footer>
  );
};
