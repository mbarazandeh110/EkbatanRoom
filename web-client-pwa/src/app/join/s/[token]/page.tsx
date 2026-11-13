"use client";

import React from "react";
import { useRouter } from "next/navigation";
import { PreJoinLobby } from "../../../../components/lobby/PreJoinLobby";
import { BrowserMediaAdapter } from "../../../../adapters/browser_media/browser_media_adapter";

interface GuestPageProps {
  params: Promise<{ token: string }>;
}

function decodeBase64Url(str: string): string {
  let base64 = str.replace(/-/g, "+").replace(/_/g, "/");
  while (base64.length % 4 !== 0) {
    base64 += "=";
  }
  if (typeof Buffer !== "undefined") {
    return Buffer.from(base64, "base64").toString("utf-8");
  }
  return decodeURIComponent(
    atob(base64)
      .split("")
      .map((c) => "%" + ("00" + c.charCodeAt(0).toString(16)).slice(-2))
      .join("")
  );
}

function GuestJoinContent({ params }: GuestPageProps) {
  const router = useRouter();
  const [token, setToken] = React.useState<string | null>(null);

  React.useEffect(() => {
    let active = true;
    Promise.resolve(params).then((p) => {
      if (active) setToken(p.token);
    });
    return () => {
      active = false;
    };
  }, [params]);

  const roomInfo = React.useMemo(() => {
    if (!token) return null;
    try {
      const base64Part = token.includes(".") ? token.split(".")[1] : token;
      const jsonPayload = decodeBase64Url(base64Part);
      const parsed = JSON.parse(jsonPayload);
      return {
        roomId: parsed.roomId || parsed.room || "guest-room",
        roomName: parsed.roomName || parsed.room || "Guest Conference",
        guestName: parsed.name || parsed.sub || "Guest Participant",
      };
    } catch {
      return {
        roomId: token,
        roomName: `Room ${token.substring(0, 8)}`,
        guestName: "Guest Participant",
      };
    }
  }, [token]);

  const mediaDevices = React.useMemo(() => new BrowserMediaAdapter(), []);

  if (!token || !roomInfo) {
    return (
      <div className="min-h-screen w-full bg-slate-950 flex items-center justify-center text-slate-400">
        <p>Verifying secure link invitation...</p>
      </div>
    );
  }

  const handleJoin = async (options: {
    participantName: string;
    audioDeviceId?: string;
    videoDeviceId?: string;
    audioOutputDeviceId?: string;
    initialCameraEnabled: boolean;
    initialMicEnabled: boolean;
  }) => {
    if (!roomInfo) return;

    // Navigate to room page with query params containing token & device choices
    const queryParams = new URLSearchParams({
      token,
      guestName: options.participantName,
      cam: String(options.initialCameraEnabled),
      mic: String(options.initialMicEnabled),
    });

    if (options.videoDeviceId) queryParams.set("videoDevice", options.videoDeviceId);
    if (options.audioDeviceId) queryParams.set("audioDevice", options.audioDeviceId);
    if (options.audioOutputDeviceId) queryParams.set("audioOutputDevice", options.audioOutputDeviceId);

    router.push(`/room/${encodeURIComponent(roomInfo.roomId)}?${queryParams.toString()}`);
  };

  return (
    <PreJoinLobby
      roomName={roomInfo.roomName}
      initialParticipantName={roomInfo.guestName}
      mediaDevices={mediaDevices}
      onJoin={handleJoin}
    />
  );
}

export default function GuestJoinPage({ params }: GuestPageProps) {
  return (
    <React.Suspense
      fallback={
        <div className="min-h-screen w-full bg-slate-950 flex items-center justify-center text-slate-400">
          <p>Verifying secure link invitation...</p>
        </div>
      }
    >
      <GuestJoinContent params={params} />
    </React.Suspense>
  );
}
