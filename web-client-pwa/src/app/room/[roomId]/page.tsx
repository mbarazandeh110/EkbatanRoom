"use client";

import React, { useState, useMemo, use } from "react";
import { useRouter } from "next/navigation";
import { useRoomStore } from "../../../store/use_room_store";
import { PreJoinLobby } from "../../../components/lobby/PreJoinLobby";
import { VideoGrid } from "../../../components/stage/VideoGrid";
import { ControlBar } from "../../../components/controls/ControlBar";
import {
  createRtcEngine,
  createMediaDevicesAdapter,
  createTelemetryAdapter,
} from "../../../adapters/factory";
import { RoomSessionController } from "../../../application/room_session_controller";
import { AdaptiveViewportManager } from "../../../application/adaptive_viewport_manager";

export default function RoomPage({
  params,
}: {
  params: Promise<{ roomId: string }>;
}) {
  const router = useRouter();
  const resolvedParams = use(params);
  const roomId = decodeURIComponent(resolvedParams.roomId);

  const status = useRoomStore((state) => state.status);

  const mediaDevices = useMemo(() => createMediaDevicesAdapter(), []);
  const telemetry = useMemo(() => createTelemetryAdapter(), []);
  const engine = useMemo(() => createRtcEngine(undefined, telemetry), [telemetry]);
  const controller = useMemo(
    () => new RoomSessionController(engine, telemetry),
    [engine, telemetry]
  );
  const viewportManager = useMemo(
    () => new AdaptiveViewportManager(engine, telemetry),
    [engine, telemetry]
  );

  const [isScreenSharing, setIsScreenSharing] = useState(false);
  const [screenTrackId, setScreenTrackId] = useState<string | null>(null);

  const handleJoin = async (options: {
    participantName: string;
    audioDeviceId?: string;
    videoDeviceId?: string;
    initialCameraEnabled: boolean;
    initialMicEnabled: boolean;
  }) => {
    // 1. Request signed token from /api/token
    const identity = `${options.participantName.toLowerCase().replace(/[^a-z0-9]/g, "-") || "user"}-${Math.random().toString(36).substring(2, 6)}`;
    const tokenRes = await fetch("/api/token", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        room: roomId,
        identity,
        name: options.participantName,
      }),
    });

    if (!tokenRes.ok) {
      throw new Error(`Failed to obtain access token: ${tokenRes.statusText}`);
    }

    const { token, url, lanIp } = await tokenRes.json();

    // 2. Connect to room engine with valid JWT
    await controller.joinRoom({
      url: url || process.env.NEXT_PUBLIC_LIVEKIT_WS_URL || "ws://localhost:7880",
      token,
      roomName: roomId,
      participantName: options.participantName,
      lanIp,
    });

    // Publish local media tracks if requested
    try {
      if (options.initialCameraEnabled || options.initialMicEnabled) {
        const stream = await mediaDevices.getUserMedia({
          videoDeviceId: options.initialCameraEnabled ? options.videoDeviceId : undefined,
          audioDeviceId: options.initialMicEnabled ? options.audioDeviceId : undefined,
        });

        const videoTrack = stream.getVideoTracks()[0];
        if (videoTrack && options.initialCameraEnabled) {
          await controller.publishLocalTrack(videoTrack, {
            source: "camera",
            simulcast: true,
          });
        }

        const audioTrack = stream.getAudioTracks()[0];
        if (audioTrack && options.initialMicEnabled) {
          await controller.publishLocalTrack(audioTrack, {
            source: "microphone",
          });
        }
      }
    } catch (err) {
      telemetry.log("warn", "media_publish_error_on_join", { error: String(err) });
    }
  };

  const handleToggleScreenShare = async () => {
    if (isScreenSharing && screenTrackId) {
      await controller.unpublishLocalTrack(screenTrackId);
      setIsScreenSharing(false);
      setScreenTrackId(null);
    } else {
      try {
        const stream = await mediaDevices.getDisplayMedia();
        const screenTrack = stream.getVideoTracks()[0];
        if (screenTrack) {
          screenTrack.onended = () => {
            setIsScreenSharing(false);
            setScreenTrackId(null);
          };
          const descriptor = await controller.publishLocalTrack(screenTrack, {
            source: "screen",
          });
          setIsScreenSharing(true);
          setScreenTrackId(descriptor.id);
        }
      } catch (err) {
        telemetry.log("warn", "screenshare_canceled_or_failed", { error: String(err) });
      }
    }
  };

  const handleLeave = async () => {
    await controller.leaveRoom();
    await mediaDevices.releaseWakeLock();
    router.push("/");
  };

  if (status !== "connected") {
    return (
      <PreJoinLobby
        roomName={roomId}
        mediaDevices={mediaDevices}
        onJoin={handleJoin}
      />
    );
  }

  return (
    <div className="flex flex-col h-screen w-screen bg-slate-950 overflow-hidden">
      {/* Top Header */}
      <header className="h-12 bg-slate-900/60 border-b border-slate-800 px-4 flex items-center justify-between text-xs text-slate-400">
        <div className="flex items-center space-x-2">
          <span className="font-semibold text-slate-200">{roomId}</span>
          <span className="px-1.5 py-0.5 rounded bg-emerald-500/20 text-emerald-400 text-[10px] font-medium uppercase tracking-wide">
            Live
          </span>
        </div>
      </header>

      {/* Main Video Grid */}
      <main className="flex-1 min-h-0 flex flex-col overflow-hidden">
        <VideoGrid viewportManager={viewportManager} />
      </main>

      {/* Bottom Control Bar */}
      <ControlBar
        controller={controller}
        onLeave={handleLeave}
        onToggleScreenShare={handleToggleScreenShare}
        isScreenSharing={isScreenSharing}
      />
    </div>
  );
}
