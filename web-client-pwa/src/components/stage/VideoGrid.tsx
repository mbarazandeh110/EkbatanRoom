"use client";

import React from "react";
import { useRoomStore } from "../../store/use_room_store";
import { useTelemetryStore } from "../../store/use_telemetry_store";
import { VideoTile } from "./VideoTile";
import { AdaptiveViewportManager } from "../../application/adaptive_viewport_manager";

export interface VideoGridProps {
  viewportManager?: AdaptiveViewportManager;
}

export const VideoGrid: React.FC<VideoGridProps> = ({ viewportManager }) => {
  const localParticipantId = useRoomStore((state) => state.localParticipantId);
  const remoteParticipantIds = useRoomStore((state) => state.remoteParticipantIds);
  const layoutMode = useRoomStore((state) => state.layoutMode);
  const activeSpeakerIds = useTelemetryStore((state) => state.activeSpeakerIds);

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

  // Speaker mode: active speaker in large view, others in bottom strip
  if (layoutMode === "speaker" && totalCount > 1) {
    const primaryId =
      activeSpeakerIds.find((id) => allParticipantIds.includes(id)) ||
      remoteParticipantIds[0] ||
      localParticipantId;

    const thumbnails = allParticipantIds.filter((id) => id !== primaryId);

    return (
      <div className="flex-1 flex flex-col p-4 gap-4 h-full overflow-hidden">
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

        {/* Thumbnail Filmstrip */}
        {thumbnails.length > 0 && (
          <div className="h-32 sm:h-40 flex gap-3 overflow-x-auto pb-1 shrink-0">
            {thumbnails.map((id) => (
              <div key={id} className="w-48 sm:w-56 h-full shrink-0">
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

  // Grid layout helper calculation
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
  );
};
