"use client";

import React, { useState, useEffect, useRef } from "react";
import { Mic, MicOff, Video, VideoOff, Sparkles, User, Volume2, Activity } from "lucide-react";
import { MediaDevicesPort } from "../../ports/media_devices_port";
import { DeviceInfo } from "../../domain/models/media";

export interface PreJoinLobbyProps {
  roomName: string;
  mediaDevices: MediaDevicesPort;
  initialParticipantName?: string;
  onJoin: (options: {
    participantName: string;
    audioDeviceId?: string;
    videoDeviceId?: string;
    audioOutputDeviceId?: string;
    initialCameraEnabled: boolean;
    initialMicEnabled: boolean;
    noiseSuppressionEnabled?: boolean;
    virtualBackgroundMode?: "none" | "blur" | "image";
  }) => Promise<void>;
}

export const PreJoinLobby: React.FC<PreJoinLobbyProps> = ({
  roomName,
  mediaDevices,
  initialParticipantName = "",
  onJoin,
}) => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [participantName, setParticipantName] = useState(initialParticipantName);
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedAudioId, setSelectedAudioId] = useState<string>("");
  const [selectedVideoId, setSelectedVideoId] = useState<string>("");
  const [selectedSpeakerId, setSelectedSpeakerId] = useState<string>("");
  const [isCameraEnabled, setIsCameraEnabled] = useState(true);
  const [isMicEnabled, setIsMicEnabled] = useState(true);
  const [noiseSuppressionEnabled, setNoiseSuppressionEnabled] = useState(true);
  const [virtualBackgroundMode, setVirtualBackgroundMode] = useState<"none" | "blur" | "image">("none");
  const [previewStream, setPreviewStream] = useState<MediaStream | null>(null);
  const [micLevel, setMicLevel] = useState<number>(0);
  const [connectionQuality, setConnectionQuality] = useState<"checking" | "optimal" | "degraded">("checking");
  const [isJoining, setIsJoining] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Check network connectivity to healthz endpoint
  useEffect(() => {
    let active = true;
    const start = Date.now();
    fetch("/api/healthz")
      .then((res) => {
        if (!active) return;
        const latency = Date.now() - start;
        setConnectionQuality(res.ok && latency < 500 ? "optimal" : "degraded");
      })
      .catch(() => {
        if (active) setConnectionQuality("degraded");
      });
    return () => {
      active = false;
    };
  }, []);

  // Enumerate devices on mount
  useEffect(() => {
    let isMounted = true;
    mediaDevices.enumerateDevices().then((devList) => {
      if (!isMounted) return;
      setDevices(devList);

      const defaultAudio = devList.find((d) => d.kind === "audioinput");
      if (defaultAudio) setSelectedAudioId(defaultAudio.deviceId);

      const defaultVideo = devList.find((d) => d.kind === "videoinput");
      if (defaultVideo) setSelectedVideoId(defaultVideo.deviceId);

      const defaultSpeaker = devList.find((d) => d.kind === "audiooutput");
      if (defaultSpeaker) setSelectedSpeakerId(defaultSpeaker.deviceId);
    });

    return () => {
      isMounted = false;
    };
  }, [mediaDevices]);

  // Real-time microphone audio level meter using Web Audio API
  useEffect(() => {
    if (!previewStream || !isMicEnabled || typeof window === "undefined") {
      setMicLevel(0);
      return;
    }
    const audioTrack = previewStream.getAudioTracks()[0];
    if (!audioTrack) return;

    const AudioCtx =
      window.AudioContext ||
      (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!AudioCtx) return;

    let ctx: AudioContext | null = null;
    let animId: number | null = null;
    let source: MediaStreamAudioSourceNode | null = null;

    try {
      ctx = new AudioCtx();
      const analyser = ctx.createAnalyser();
      analyser.fftSize = 64;
      const dataArray = new Uint8Array(analyser.frequencyBinCount);
      const audioStream = new MediaStream([audioTrack]);
      source = ctx.createMediaStreamSource(audioStream);
      source.connect(analyser);

      const updateMeter = () => {
        analyser.getByteFrequencyData(dataArray);
        let sum = 0;
        for (let i = 0; i < dataArray.length; i++) {
          sum += dataArray[i];
        }
        const avg = sum / dataArray.length;
        setMicLevel(Math.min(1, avg / 128));
        animId = requestAnimationFrame(updateMeter);
      };
      animId = requestAnimationFrame(updateMeter);
    } catch {
      // Audio meter fallback
    }

    return () => {
      if (animId) cancelAnimationFrame(animId);
      if (source) source.disconnect();
      if (ctx && ctx.state !== "closed") {
        ctx.close().catch(() => {});
      }
    };
  }, [previewStream, isMicEnabled]);

  // Handle local camera preview
  useEffect(() => {
    let stream: MediaStream | null = null;
    let isCancelled = false;

    if (isCameraEnabled) {
      mediaDevices
        .getUserMedia({
          videoDeviceId: selectedVideoId || undefined,
          audioDeviceId: selectedAudioId || undefined,
        })
        .then((s) => {
          if (isCancelled) {
            mediaDevices.stopStream(s);
            return;
          }
          stream = s;
          setPreviewStream(s);
          if (videoRef.current) {
            videoRef.current.srcObject = s;
          }
        })
        .catch(() => {
          if (!isCancelled) {
            setError("Could not access camera or microphone preview.");
          }
        });
    } else {
      setPreviewStream(null);
      if (videoRef.current) {
        videoRef.current.srcObject = null;
      }
    }

    return () => {
      isCancelled = true;
      if (stream) {
        mediaDevices.stopStream(stream);
      }
    };
  }, [mediaDevices, isCameraEnabled, selectedVideoId, selectedAudioId]);

  const handleJoinClick = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!participantName.trim() || isJoining) return;

    setIsJoining(true);
    setError(null);

    try {
      // 1. Mandatory iOS Safari AudioContext unlock
      await mediaDevices.unlockAudioContext();

      // 2. Request WakeLock to prevent screen lock
      await mediaDevices.requestWakeLock();

      // 3. Stop preview stream before entering room
      if (previewStream) {
        mediaDevices.stopStream(previewStream);
        setPreviewStream(null);
      }

      await onJoin({
        participantName: participantName.trim(),
        audioDeviceId: selectedAudioId || undefined,
        videoDeviceId: selectedVideoId || undefined,
        audioOutputDeviceId: selectedSpeakerId || undefined,
        initialCameraEnabled: isCameraEnabled,
        initialMicEnabled: isMicEnabled,
        noiseSuppressionEnabled,
        virtualBackgroundMode,
      });
    } catch (err: unknown) {
      setIsJoining(false);
      setError(err instanceof Error ? err.message : "Failed to join room.");
    }
  };

  const audioDevices = devices.filter((d) => d.kind === "audioinput");
  const videoDevices = devices.filter((d) => d.kind === "videoinput");
  const speakerDevices = devices.filter((d) => d.kind === "audiooutput");

  return (
    <div className="min-h-screen w-full bg-slate-950 flex flex-col items-center justify-center p-4">
      <div className="w-full max-w-4xl bg-slate-900 border border-slate-800 rounded-2xl shadow-2xl overflow-hidden flex flex-col md:flex-row">
        {/* Left: Video Preview */}
        <div className="w-full md:w-1/2 p-6 flex flex-col items-center justify-center bg-slate-950/60 border-b md:border-b-0 md:border-r border-slate-800">
          <div className="relative w-full aspect-video rounded-xl bg-slate-900 overflow-hidden border border-slate-800 flex items-center justify-center shadow-inner">
            <video
              ref={videoRef}
              playsInline
              autoPlay
              muted
              className={`w-full h-full object-cover -scale-x-100 ${
                isCameraEnabled ? "block" : "hidden"
              }`}
            />

            {!isCameraEnabled && (
              <div className="flex flex-col items-center text-slate-500">
                <User className="w-16 h-16 text-slate-600 mb-2" />
                <span className="text-sm">Camera is turned off</span>
              </div>
            )}

            {/* Quick toggles on preview */}
            <div className="absolute bottom-3 flex space-x-2">
              <button
                type="button"
                onClick={() => setIsMicEnabled(!isMicEnabled)}
                aria-label={isMicEnabled ? "Mute preview microphone" : "Unmute preview microphone"}
                className={`p-2.5 rounded-full backdrop-blur-md transition-all ${
                  isMicEnabled
                    ? "bg-slate-800/80 text-white hover:bg-slate-700/80"
                    : "bg-red-500/80 text-white"
                }`}
              >
                {isMicEnabled ? <Mic className="w-4 h-4" /> : <MicOff className="w-4 h-4" />}
              </button>

              <button
                type="button"
                onClick={() => setIsCameraEnabled(!isCameraEnabled)}
                aria-label={isCameraEnabled ? "Turn off preview camera" : "Turn on preview camera"}
                className={`p-2.5 rounded-full backdrop-blur-md transition-all ${
                  isCameraEnabled
                    ? "bg-slate-800/80 text-white hover:bg-slate-700/80"
                    : "bg-red-500/80 text-white"
                }`}
              >
                {isCameraEnabled ? <Video className="w-4 h-4" /> : <VideoOff className="w-4 h-4" />}
              </button>
            </div>
          </div>

          {/* Device Selectors & Audio Meter */}
          <div className="w-full mt-4 space-y-3 text-xs text-slate-400">
            {videoDevices.length > 0 && (
              <div>
                <label className="block mb-1 font-medium text-slate-300">Camera</label>
                <select
                  value={selectedVideoId}
                  onChange={(e) => setSelectedVideoId(e.target.value)}
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-emerald-500"
                >
                  {videoDevices.map((d) => (
                    <option key={d.deviceId} value={d.deviceId}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </div>
            )}

            {audioDevices.length > 0 && (
              <div>
                <div className="flex items-center justify-between mb-1">
                  <label className="font-medium text-slate-300">Microphone</label>
                  {isMicEnabled && (
                    <span className="text-[10px] text-slate-400">Mic Level</span>
                  )}
                </div>
                <select
                  value={selectedAudioId}
                  onChange={(e) => setSelectedAudioId(e.target.value)}
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-emerald-500 mb-1.5"
                >
                  {audioDevices.map((d) => (
                    <option key={d.deviceId} value={d.deviceId}>
                      {d.label}
                    </option>
                  ))}
                </select>

                {/* Real-time Mic Level Bar */}
                {isMicEnabled && (
                  <div
                    data-testid="mic-level-meter"
                    className="w-full h-1.5 bg-slate-800 rounded-full overflow-hidden border border-slate-700/50"
                  >
                    <div
                      className="h-full bg-gradient-to-r from-emerald-500 via-teal-400 to-cyan-400 transition-all duration-75"
                      style={{ width: `${Math.round(micLevel * 100)}%` }}
                    />
                  </div>
                )}
              </div>
            )}

            {speakerDevices.length > 0 && (
              <div>
                <div className="flex items-center space-x-1 mb-1 font-medium text-slate-300">
                  <Volume2 className="w-3.5 h-3.5 text-slate-400" />
                  <label>Speaker / Audio Output</label>
                </div>
                <select
                  value={selectedSpeakerId}
                  onChange={(e) => setSelectedSpeakerId(e.target.value)}
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-emerald-500"
                >
                  {speakerDevices.map((d) => (
                    <option key={d.deviceId} value={d.deviceId}>
                      {d.label}
                    </option>
                  ))}
                </select>
              </div>
            )}

            {/* DSP Controls: Noise Suppression & Virtual Background */}
            <div className="pt-2 border-t border-slate-800 space-y-2">
              <label className="flex items-center space-x-2 cursor-pointer text-slate-300 hover:text-white transition-colors">
                <input
                  type="checkbox"
                  checked={noiseSuppressionEnabled}
                  onChange={(e) => setNoiseSuppressionEnabled(e.target.checked)}
                  className="rounded bg-slate-800 border-slate-700 text-emerald-500 focus:ring-emerald-500"
                />
                <span className="text-[11px] font-medium">Enable AI Noise Suppression (DSP)</span>
              </label>

              <div>
                <label className="block mb-1 text-[11px] font-medium text-slate-300">Virtual Background</label>
                <select
                  value={virtualBackgroundMode}
                  onChange={(e) => setVirtualBackgroundMode(e.target.value as "none" | "blur" | "image")}
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-2.5 py-1.5 text-slate-200 focus:outline-none focus:ring-1 focus:ring-emerald-500 text-xs"
                >
                  <option value="none">None (Standard Video)</option>
                  <option value="blur">Blur Background (Studio)</option>
                  <option value="image">Office Background</option>
                </select>
              </div>
            </div>
          </div>
        </div>

        {/* Right: Meeting Info & Join Action */}
        <div className="w-full md:w-1/2 p-6 md:p-8 flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center space-x-2 text-emerald-400 text-sm font-semibold">
                <Sparkles className="w-4 h-4" />
                <span>EkbatanRoom Conference</span>
              </div>
              <div
                className={`flex items-center space-x-1.5 px-2 py-0.5 rounded-full text-[11px] font-medium border ${
                  connectionQuality === "optimal"
                    ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20"
                    : connectionQuality === "degraded"
                    ? "bg-amber-500/10 text-amber-400 border-amber-500/20"
                    : "bg-slate-800 text-slate-400 border-slate-700"
                }`}
              >
                <Activity className="w-3 h-3" />
                <span>{connectionQuality === "optimal" ? "Network Ready" : connectionQuality === "degraded" ? "Degraded Net" : "Checking..."}</span>
              </div>
            </div>
            <h1 className="text-2xl font-bold text-white mb-1">{roomName}</h1>
            <p className="text-sm text-slate-400 mb-6">
              Check your audio and video before entering the meeting room.
            </p>

            {error && (
              <div className="mb-4 p-3 bg-red-950/60 border border-red-800 rounded-lg text-xs text-red-300">
                {error}
              </div>
            )}

            <form onSubmit={handleJoinClick} className="space-y-4">
              <div>
                <label htmlFor="name-input" className="block text-sm font-medium text-slate-300 mb-1">
                  Your Name
                </label>
                <input
                  id="name-input"
                  type="text"
                  required
                  placeholder="e.g. Mahdi"
                  value={participantName}
                  onChange={(e) => setParticipantName(e.target.value)}
                  className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3.5 py-2.5 text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent transition-all"
                />
              </div>

              <button
                type="submit"
                disabled={!participantName.trim() || isJoining}
                className="w-full mt-4 bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 disabled:cursor-not-allowed text-white font-semibold py-3 px-4 rounded-xl shadow-lg shadow-emerald-600/30 transition-all focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:ring-offset-2 focus:ring-offset-slate-900"
              >
                {isJoining ? "Entering room..." : "Join Room"}
              </button>
            </form>
          </div>

          <div className="mt-8 text-center text-xs text-slate-500">
            Powered by Next.js & Dual-Protocol WebRTC Hexagonal Core
          </div>
        </div>
      </div>
    </div>
  );
};
