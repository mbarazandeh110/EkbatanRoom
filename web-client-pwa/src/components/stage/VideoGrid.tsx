"use client";

import React from "react";
import { useRoomStore } from "../../store/use_room_store";
import { useTelemetryStore } from "../../store/use_telemetry_store";
import { useParticipantsStore } from "../../store/use_participants_store";
import { VideoTile } from "./VideoTile";
import { DynamicForensicWatermark } from "./DynamicForensicWatermark";
import { AdaptiveViewportManager } from "../../application/adaptive_viewport_manager";
import { LayoutMode } from "../../domain/models/room";
import { Lock } from "lucide-react";

export interface VideoGridProps {
  viewportManager?: AdaptiveViewportManager;
  overrideLayoutMode?: LayoutMode;
  spotlightParticipantId?: string | null;
  isHostSyncLocked?: boolean;
  watermarkEnabled?: boolean;
  watermarkUserPhone?: string;
}

export const VideoGrid: React.FC<VideoGridProps> = ({
  viewportManager,
  overrideLayoutMode,
  spotlightParticipantId,
  isHostSyncLocked = false,
  watermarkEnabled = true,
  watermarkUserPhone,
}) => {
  const localParticipantId = useRoomStore((state) => state.localParticipantId);
  const remoteParticipantIds = useRoomStore((state) => state.remoteParticipantIds);
  const storeLayoutMode = useRoomStore((state) => state.layoutMode);
  const activeSpeakerIds = useTelemetryStore((state) => state.activeSpeakerIds);

  const localParticipant = useParticipantsStore((state) =>
    localParticipantId ? state.participants[localParticipantId] : undefined
  );

  const currentLayoutMode = overrideLayoutMode || storeLayoutMode;

  const allParticipantIds = [
    ...(localParticipantId ? [localParticipantId] : []),
    ...remoteParticipantIds,
  ];

  const totalCount = allParticipantIds.length;

  if (totalCount === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-slate-500">
        <p>Waiting for participants to join...</p>
      </div>
    );
  }

  // 1. Presentation Mode: large presentation stage on left + vertical filmstrip on right
  if (currentLayoutMode === "presentation" && totalCount > 1) {
    const primaryId =
      spotlightParticipantId ||
      activeSpeakerIds.find((id) => allParticipantIds.includes(id)) ||
      remoteParticipantIds[0] ||
      localParticipantId;

    const thumbnails = allParticipantIds.filter((id) => id !== primaryId);

    return (
      <div className="relative flex-1 flex flex-col md:flex-row p-3 sm:p-4 gap-3 sm:gap-4 h-full overflow-hidden">
        {/* Host Sync Indicator */}
        {isHostSyncLocked && (
          <div className="absolute top-5 left-5 z-30 flex items-center space-x-1.5 bg-indigo-600/90 text-white text-[11px] font-semibold px-2.5 py-1 rounded-full shadow-lg backdrop-blur-md">
            <Lock className="w-3 h-3" />
            <span>Host Locked Presentation</span>
          </div>
        )}

        {/* Dynamic Forensic Watermark */}
        {watermarkEnabled && (
          <DynamicForensicWatermark
            participantName={localParticipant?.name || "Participant"}
            userId={localParticipantId || undefined}
            userPhone={watermarkUserPhone}
          />
        )}

        {/* Main Presentation View */}
        <div className="flex-1 min-h-0 min-w-0">
          {primaryId && (
            <VideoTile
              participantId={primaryId}
              isLocal={primaryId === localParticipantId}
              viewportManager={viewportManager}
            />
          )}
        </div>

        {/* Vertical Side Filmstrip */}
        {thumbnails.length > 0 && (
          <div className="w-full md:w-64 lg:w-72 flex md:flex-col gap-3 overflow-x-auto md:overflow-y-auto shrink-0 pb-1 md:pb-0">
            {thumbnails.map((id) => (
              <div key={id} className="w-48 sm:w-56 md:w-full h-32 md:h-40 shrink-0">
                <VideoTile
                  participantId={id}
                  isLocal={id === localParticipantId}
                  viewportManager={viewportManager}
                />
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // 2. Speaker Mode: active speaker in large view + horizontal filmstrip below
  if (currentLayoutMode === "speaker" && totalCount > 1) {
    const primaryId =
      spotlightParticipantId ||
      activeSpeakerIds.find((id) => allParticipantIds.includes(id)) ||
      remoteParticipantIds[0] ||
      localParticipantId;

    const thumbnails = allParticipantIds.filter((id) => id !== primaryId);

    return (
      <div className="relative flex-1 flex flex-col p-3 sm:p-4 gap-3 sm:gap-4 h-full overflow-hidden">
        {isHostSyncLocked && (
          <div className="absolute top-5 left-5 z-30 flex items-center space-x-1.5 bg-indigo-600/90 text-white text-[11px] font-semibold px-2.5 py-1 rounded-full shadow-lg backdrop-blur-md">
            <Lock className="w-3 h-3" />
            <span>Host Locked Spotlight</span>
          </div>
        )}

        {watermarkEnabled && (
          <DynamicForensicWatermark
            participantName={localParticipant?.name || "Participant"}
            userId={localParticipantId || undefined}
            userPhone={watermarkUserPhone}
          />
        )}

        {/* Main Speaker Stage */}
        <div className="flex-1 min-h-0">
          {primaryId && (
            <VideoTile
              participantId={primaryId}
              isLocal={primaryId === localParticipantId}
              viewportManager={viewportManager}
            />
          )}
        </div>

        {/* Bottom Thumbnail Filmstrip */}
        {thumbnails.length > 0 && (
          <div className="h-28 sm:h-36 flex gap-3 overflow-x-auto pb-1 shrink-0">
            {thumbnails.map((id) => (
              <div key={id} className="w-44 sm:w-52 h-full shrink-0">
                <VideoTile
                  participantId={id}
                  isLocal={id === localParticipantId}
                  viewportManager={viewportManager}
                />
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  // 3. Grid Mode: standard gallery grid
  const getGridClasses = (count: number) => {
    if (count === 1) return "grid-cols-1 grid-rows-1";
    if (count === 2) return "grid-cols-1 md:grid-cols-2 grid-rows-1 md:grid-rows-1";
    if (count <= 4) return "grid-cols-2 grid-rows-2";
    if (count <= 6) return "grid-cols-2 md:grid-cols-3 grid-rows-3 md:grid-rows-2";
    if (count <= 9) return "grid-cols-3 grid-rows-3";
    return "grid-cols-3 md:grid-cols-4";
  };

  return (
    <div
      data-testid="video-grid"
      className="relative flex-1 h-full w-full overflow-hidden flex flex-col"
    >
      {watermarkEnabled && (
        <DynamicForensicWatermark
          participantName={localParticipant?.name || "Participant"}
          userId={localParticipantId || undefined}
          userPhone={watermarkUserPhone}
        />
      )}

      {isHostSyncLocked && (
        <div className="absolute top-5 left-5 z-30 flex items-center space-x-1.5 bg-indigo-600/90 text-white text-[11px] font-semibold px-2.5 py-1 rounded-full shadow-lg backdrop-blur-md">
          <Lock className="w-3 h-3" />
          <span>Host Locked Grid</span>
        </div>
      )}

      <div
        className={`flex-1 grid gap-3 sm:gap-4 p-3 sm:p-4 h-full w-full overflow-hidden ${getGridClasses(
          totalCount
        )}`}
      >
        {allParticipantIds.map((id) => (
          <div key={id} className="w-full h-full min-h-0 min-w-0">
            <VideoTile
              participantId={id}
              isLocal={id === localParticipantId}
              viewportManager={viewportManager}
            />
          </div>
        ))}
      </div>
    </div>
  );
};
